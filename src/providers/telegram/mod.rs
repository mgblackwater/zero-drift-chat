pub mod convert;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use futures::future::BoxFuture;
use grammers_client::client::UpdatesConfiguration;
use grammers_client::peer::Peer;
use grammers_client::{Client, SenderPool, SignInError};
use grammers_session::{Session, SessionData};
use grammers_session::types::{ChannelKind, DcOption, PeerId, PeerInfo, UpdateState, UpdatesState};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex as TokioMutex};
use tokio::task::JoinHandle;

use crate::core::error::Result;
use crate::core::provider::{MessagingProvider, ProviderEvent};
use crate::core::types::*;

use convert::{grammers_message_to_unified, peer_id_to_chat_id, PeerCache};

use grammers_client::client::PasswordToken;

const MAX_DIALOGS: usize = 200;
const MAX_AUTH_RETRIES: u8 = 3;

/// Messages sent *into* the provider during interactive auth.
pub enum AuthInput {
    Phone(String),
    Otp(String),
    Password(String),
}

// ---------------------------------------------------------------------------
// JSON-serializable mirror of SessionData (SessionData itself lacks serde).
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct SessionSnapshot {
    home_dc: i32,
    dc_options: HashMap<i32, DcOption>,
    peer_infos: HashMap<PeerId, PeerInfo>,
    updates_state: UpdatesState,
}

impl From<&SessionData> for SessionSnapshot {
    fn from(d: &SessionData) -> Self {
        Self {
            home_dc: d.home_dc,
            dc_options: d.dc_options.clone(),
            peer_infos: d.peer_infos.clone(),
            updates_state: d.updates_state.clone(),
        }
    }
}

impl From<SessionSnapshot> for SessionData {
    fn from(s: SessionSnapshot) -> Self {
        Self {
            home_dc: s.home_dc,
            dc_options: s.dc_options,
            peer_infos: s.peer_infos,
            updates_state: s.updates_state,
        }
    }
}

// ---------------------------------------------------------------------------
// Persistent session: wraps SessionData in an Arc<RwLock> so we can snapshot
// it to JSON at any time while also satisfying the grammers Session trait.
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct PersistentSession(Arc<RwLock<SessionData>>);

impl PersistentSession {
    pub fn new(data: SessionData) -> Self {
        Self(Arc::new(RwLock::new(data)))
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot::from(&*self.0.read().unwrap())
    }
}

impl Session for PersistentSession {
    fn home_dc_id(&self) -> i32 {
        self.0.read().unwrap().home_dc
    }

    fn set_home_dc_id(&self, dc_id: i32) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.0.write().unwrap().home_dc = dc_id;
        })
    }

    fn dc_option(&self, dc_id: i32) -> Option<DcOption> {
        self.0.read().unwrap().dc_options.get(&dc_id).cloned()
    }

    fn set_dc_option(&self, dc_option: &DcOption) -> BoxFuture<'_, ()> {
        let dc_option = dc_option.clone();
        Box::pin(async move {
            self.0
                .write()
                .unwrap()
                .dc_options
                .insert(dc_option.id, dc_option);
        })
    }

    fn peer(&self, peer: PeerId) -> BoxFuture<'_, Option<PeerInfo>> {
        Box::pin(async move {
            self.0.read().unwrap().peer_infos.get(&peer).cloned()
        })
    }

    fn cache_peer(&self, peer: &PeerInfo) -> BoxFuture<'_, ()> {
        let peer = peer.clone();
        Box::pin(async move {
            use grammers_session::types::PeerId as SessionPeerId;
            let id: SessionPeerId = peer.id();
            self.0
                .write()
                .unwrap()
                .peer_infos
                .entry(id)
                .or_insert_with(|| peer.clone())
                .extend_info(&peer);
        })
    }

    fn updates_state(&self) -> BoxFuture<'_, UpdatesState> {
        Box::pin(async move { self.0.read().unwrap().updates_state.clone() })
    }

    fn set_update_state(&self, update: UpdateState) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let mut data = self.0.write().unwrap();
            match update {
                UpdateState::All(updates_state) => {
                    data.updates_state = updates_state;
                }
                UpdateState::Primary { pts, date, seq } => {
                    data.updates_state.pts = pts;
                    data.updates_state.date = date;
                    data.updates_state.seq = seq;
                }
                UpdateState::Secondary { qts } => {
                    data.updates_state.qts = qts;
                }
                UpdateState::Channel { id, pts } => {
                    use grammers_session::types::ChannelState;
                    data.updates_state.channels.retain(|c| c.id != id);
                    data.updates_state.channels.push(ChannelState { id, pts });
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Session load / save helpers.
// ---------------------------------------------------------------------------

fn load_session(path: &PathBuf) -> PersistentSession {
    if let Ok(bytes) = std::fs::read(path) {
        match serde_json::from_slice::<SessionSnapshot>(&bytes) {
            Ok(snapshot) => {
                tracing::info!("Telegram: loaded session from {}", path.display());
                return PersistentSession::new(SessionData::from(snapshot));
            }
            Err(e) => {
                tracing::warn!(
                    "Telegram: failed to parse session JSON ({}), starting fresh",
                    e
                );
            }
        }
    } else {
        tracing::info!("Telegram: no existing session file, starting fresh");
    }
    PersistentSession::new(SessionData::default())
}

fn save_session(session: &PersistentSession, path: &PathBuf) {
    let snapshot = session.snapshot();
    match serde_json::to_vec_pretty(&snapshot) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(path, &bytes) {
                tracing::error!(
                    "Telegram: failed to write session to {}: {}",
                    path.display(),
                    e
                );
            } else {
                tracing::info!("Telegram: session saved to {}", path.display());
            }
        }
        Err(e) => {
            tracing::error!("Telegram: failed to serialize session: {}", e);
        }
    }
}

// ---------------------------------------------------------------------------
// Provider struct.
// ---------------------------------------------------------------------------

pub struct TelegramProvider {
    api_id: i32,
    api_hash: String,
    session_path: PathBuf,

    /// Shared client handle — set by the background task once connected.
    client: Arc<TokioMutex<Option<Client>>>,
    peer_cache: PeerCache,
    auth_status: AuthStatus,

    /// Channel the app uses to push auth answers back into the running start() future.
    pub auth_tx: mpsc::UnboundedSender<AuthInput>,
    auth_rx: Option<mpsc::UnboundedReceiver<AuthInput>>,

    /// Background task handle (runs connect+auth+update loop).
    runner_handle: Option<JoinHandle<()>>,
}

impl TelegramProvider {
    pub fn new(api_id: i32, api_hash: String, session_path: String) -> Self {
        let (auth_tx, auth_rx) = mpsc::unbounded_channel();
        Self {
            api_id,
            api_hash,
            session_path: PathBuf::from(session_path),
            client: Arc::new(TokioMutex::new(None)),
            peer_cache: PeerCache::new(),
            auth_status: AuthStatus::NotAuthenticated,
            auth_tx,
            auth_rx: Some(auth_rx),
            runner_handle: None,
        }
    }

    /// Perform the interactive authentication flow.
    async fn authenticate(
        client: &Client,
        api_hash: &str,
        auth_rx: &mut mpsc::UnboundedReceiver<AuthInput>,
        tx: &mpsc::UnboundedSender<ProviderEvent>,
    ) -> Result<()> {
        // Prompt for phone number.
        let _ = tx.send(ProviderEvent::AuthPhonePrompt(Platform::Telegram, None));
        let phone = loop {
            match auth_rx.recv().await {
                Some(AuthInput::Phone(p)) => break p,
                None => {
                    return Err(anyhow::anyhow!(
                        "Auth channel closed before phone was provided"
                    ))
                }
                _ => {}
            }
        };

        let login_token = client
            .request_login_code(&phone, api_hash)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to request login code: {}", e))?;

        // Prompt for OTP; retry on invalid code.
        let _ = tx.send(ProviderEvent::AuthOtpPrompt(Platform::Telegram, None));
        let mut password_token: Option<PasswordToken> = None;
        let mut otp_retries = 0u8;
        loop {
            let code = loop {
                match auth_rx.recv().await {
                    Some(AuthInput::Otp(c)) => break c,
                    None => {
                        return Err(anyhow::anyhow!(
                            "Auth channel closed before OTP was provided"
                        ))
                    }
                    _ => {}
                }
            };

            match client.sign_in(&login_token, &code).await {
                Ok(_user) => {
                    tracing::info!("Telegram: signed in with OTP");
                    return Ok(());
                }
                Err(SignInError::PasswordRequired(token)) => {
                    password_token = Some(token);
                    break;
                }
                Err(SignInError::InvalidCode) => {
                    otp_retries += 1;
                    if otp_retries >= MAX_AUTH_RETRIES {
                        let _ = tx.send(ProviderEvent::AuthStatusChanged(
                            Platform::Telegram,
                            AuthStatus::Failed,
                        ));
                        return Err(anyhow::anyhow!("Too many wrong OTP attempts"));
                    }
                    let _ = tx.send(ProviderEvent::AuthOtpPrompt(
                        Platform::Telegram,
                        Some("Wrong code, try again".to_string()),
                    ));
                }
                Err(e) => return Err(anyhow::anyhow!("sign_in error: {}", e)),
            }
        }

        // 2FA password loop.
        let mut pt = password_token.unwrap();
        let _ = tx.send(ProviderEvent::AuthPasswordPrompt(Platform::Telegram, None));
        let mut pw_retries = 0u8;
        loop {
            let password = loop {
                match auth_rx.recv().await {
                    Some(AuthInput::Password(p)) => break p,
                    None => {
                        return Err(anyhow::anyhow!(
                            "Auth channel closed before password was provided"
                        ))
                    }
                    _ => {}
                }
            };

            match client.check_password(pt, password.as_bytes()).await {
                Ok(_user) => {
                    tracing::info!("Telegram: signed in with 2FA password");
                    return Ok(());
                }
                Err(SignInError::InvalidPassword(new_token)) => {
                    pt = new_token;
                    pw_retries += 1;
                    if pw_retries >= MAX_AUTH_RETRIES {
                        let _ = tx.send(ProviderEvent::AuthStatusChanged(
                            Platform::Telegram,
                            AuthStatus::Failed,
                        ));
                        return Err(anyhow::anyhow!("Too many wrong 2FA password attempts"));
                    }
                    let _ = tx.send(ProviderEvent::AuthPasswordPrompt(
                        Platform::Telegram,
                        Some("Wrong password, try again".to_string()),
                    ));
                }
                Err(e) => return Err(anyhow::anyhow!("check_password error: {}", e)),
            }
        }
    }

    /// Background task: connect, authenticate, fetch initial dialogs, then run the update loop.
    /// Called from within the tokio::spawn in `start()`.
    async fn connect_and_run(
        api_id: i32,
        api_hash: String,
        session_path: PathBuf,
        peer_cache: PeerCache,
        client_slot: Arc<TokioMutex<Option<Client>>>,
        mut auth_rx: mpsc::UnboundedReceiver<AuthInput>,
        tx: mpsc::UnboundedSender<ProviderEvent>,
    ) -> Result<()> {
        // 1. Load (or create fresh) session.
        let session = Arc::new(load_session(&session_path));

        // 2. Build the sender pool (network I/O layer).
        let pool = SenderPool::new(Arc::clone(&session), api_id);

        // 3. Build client from the pool handle (thin wrapper, no network yet).
        let client = Client::new(pool.handle);

        // 4. Spawn the network runner — this drives all MTProto I/O.
        //    We hold the runner in a task; `updates_rx` belongs to us.
        let updates_rx = pool.updates;
        let runner_handle = tokio::spawn(async move {
            pool.runner.run().await;
        });

        // 5. Authenticate if not already logged in.
        match client.is_authorized().await {
            Ok(false) => {
                tracing::info!("Telegram: not authorized, starting auth flow");
                Self::authenticate(&client, &api_hash, &mut auth_rx, &tx).await?;
            }
            Ok(true) => {
                tracing::info!("Telegram: already authorized (session loaded)");
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Telegram: is_authorized failed: {}", e));
            }
        }

        // Store the client so callers (send_message, get_messages, etc.) can use it.
        *client_slot.lock().await = Some(client.clone());

        // 6. Persist session after successful auth.
        save_session(&session, &session_path);

        // 7. Notify TUI that we are authenticated.
        let _ = tx.send(ProviderEvent::AuthStatusChanged(
            Platform::Telegram,
            AuthStatus::Authenticated,
        ));

        // 8. Fetch initial dialog list and send to TUI.
        let mut dialogs = client.iter_dialogs();
        let mut chats = Vec::new();
        let mut count = 0usize;

        while let Some(dialog) = dialogs
            .next()
            .await
            .map_err(|e| anyhow::anyhow!("iter_dialogs error: {}", e))?
        {
            if count >= MAX_DIALOGS {
                break;
            }
            count += 1;

            let peer = dialog.peer();
            let peer_ref = dialog.peer_ref();
            let peer_id = peer.id();

            let chat_id_str = peer_id
                .bot_api_dialog_id()
                .map(peer_id_to_chat_id)
                .unwrap_or_else(|| "tg-unknown".to_string());

            peer_cache.insert(&chat_id_str, peer_ref);

            let name = peer.name().unwrap_or("Unknown").to_string();
            let last_message = dialog.last_message.as_ref().map(|m| {
                if m.text().is_empty() {
                    "[Media]".to_string()
                } else {
                    m.text().to_string()
                }
            });
            let kind = match peer {
                Peer::User(u) if u.is_bot() => ChatKind::Bot,
                Peer::User(_)               => ChatKind::Chat,
                Peer::Group(_)              => ChatKind::Group,
                Peer::Channel(c) if matches!(c.kind(), Some(ChannelKind::Megagroup | ChannelKind::Gigagroup)) => ChatKind::Group,
                Peer::Channel(_)            => ChatKind::Channel,
            };

            chats.push(UnifiedChat {
                id: chat_id_str,
                platform: Platform::Telegram,
                name,
                display_name: None,
                last_message,
                unread_count: 0,
                kind,
                is_pinned: false,
                is_muted: false,
            });
        }

        let _ = tx.send(ProviderEvent::ChatsUpdated(chats));
        let _ = tx.send(ProviderEvent::SyncCompleted);
        save_session(&session, &session_path);

        // 9. Run the update loop — stream incoming updates until the runner exits.
        let mut update_stream = client
            .stream_updates(updates_rx, UpdatesConfiguration::default())
            .await;

        loop {
            match update_stream.next().await {
                Ok(update) => {
                    if let grammers_client::update::Update::NewMessage(msg) = update {
                        let chat_id = msg
                            .peer_id()
                            .bot_api_dialog_id()
                            .map(peer_id_to_chat_id)
                            .unwrap_or_else(|| "tg-unknown".to_string());

                        if let Some(unified) = grammers_message_to_unified(&msg, &chat_id) {
                            let _ = tx.send(ProviderEvent::NewMessage(unified));
                        }
                    }
                    // Other update kinds are ignored for now.
                }
                Err(e) => {
                    tracing::warn!("Telegram update stream error: {}; stopping", e);
                    break;
                }
            }
        }

        // 10. Clean up runner.
        runner_handle.abort();
        Ok(())
    }
}

#[async_trait]
impl MessagingProvider for TelegramProvider {
    async fn start(&mut self, tx: mpsc::UnboundedSender<ProviderEvent>) -> Result<()> {
        self.auth_status = AuthStatus::Authenticating;
        let _ = tx.send(ProviderEvent::AuthStatusChanged(
            Platform::Telegram,
            AuthStatus::Authenticating,
        ));

        let api_id = self.api_id;
        let api_hash = self.api_hash.clone();
        let session_path = self.session_path.clone();
        let peer_cache = self.peer_cache.clone();
        let client_slot = Arc::clone(&self.client);
        let auth_rx = self
            .auth_rx
            .take()
            .ok_or_else(|| anyhow::anyhow!("auth_rx already consumed"))?;

        // Spawn the entire connect+auth+update loop as a background task so that
        // start() returns immediately and the TUI can draw before auth is needed.
        let connect_handle = tokio::spawn(async move {
            if let Err(e) = Self::connect_and_run(
                api_id, api_hash, session_path, peer_cache, client_slot, auth_rx, tx.clone(),
            )
            .await
            {
                tracing::error!("Telegram background task failed: {}", e);
                let _ = tx.send(ProviderEvent::AuthStatusChanged(
                    Platform::Telegram,
                    AuthStatus::Failed,
                ));
            }
        });

        self.runner_handle = Some(connect_handle);
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(h) = self.runner_handle.take() {
            h.abort();
        }
        // Disconnect the client if it was set.
        if let Some(client) = self.client.lock().await.as_ref() {
            client.disconnect();
        }
        *self.client.lock().await = None;
        self.auth_status = AuthStatus::NotAuthenticated;
        Ok(())
    }

    async fn send_message(&self, chat_id: &str, content: MessageContent) -> Result<UnifiedMessage> {
        let guard = self.client.lock().await;
        let client = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Telegram client not started"))?;

        let peer = self
            .peer_cache
            .get(chat_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown chat_id: {}", chat_id))?;

        let text = match &content {
            MessageContent::Text(t) => t.clone(),
            other => other.as_text().to_string(),
        };

        let sent = client
            .send_message(peer, text.as_str())
            .await
            .map_err(|e| anyhow::anyhow!("send_message failed: {}", e))?;

        Ok(UnifiedMessage {
            id: sent.id().to_string(),
            chat_id: chat_id.to_string(),
            platform: Platform::Telegram,
            sender: "You".to_string(),
            content,
            timestamp: sent.date(),
            status: MessageStatus::Sent,
            is_outgoing: true,
        })
    }

    async fn get_chats(&self) -> Result<Vec<UnifiedChat>> {
        let client = self.client.lock().await
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Telegram client not started"))?;

        let mut dialogs = client.iter_dialogs();
        let mut chats = Vec::new();
        let mut count = 0;

        while let Some(dialog) = dialogs
            .next()
            .await
            .map_err(|e| anyhow::anyhow!("iter_dialogs error: {}", e))?
        {
            if count >= MAX_DIALOGS {
                break;
            }
            count += 1;
            let peer = dialog.peer();
            let peer_ref = dialog.peer_ref();
            let peer_id = peer.id();

            let chat_id_str = peer_id
                .bot_api_dialog_id()
                .map(peer_id_to_chat_id)
                .unwrap_or_else(|| "tg-unknown".to_string());

            self.peer_cache.insert(&chat_id_str, peer_ref);

            let name = peer.name().unwrap_or("Unknown").to_string();

            let last_message = dialog.last_message.as_ref().map(|m| {
                if m.text().is_empty() {
                    "[Media]".to_string()
                } else {
                    m.text().to_string()
                }
            });

            let kind = match peer {
                Peer::User(u) if u.is_bot() => ChatKind::Bot,
                Peer::User(_)               => ChatKind::Chat,
                Peer::Group(_)              => ChatKind::Group,
                Peer::Channel(c) if matches!(c.kind(), Some(ChannelKind::Megagroup | ChannelKind::Gigagroup)) => ChatKind::Group,
                Peer::Channel(_)            => ChatKind::Channel,
            };

            chats.push(UnifiedChat {
                id: chat_id_str,
                platform: Platform::Telegram,
                name,
                display_name: None,
                last_message,
                unread_count: 0,
                kind,
                is_pinned: false,
                is_muted: false,
            });
        }

        Ok(chats)
    }

    async fn get_messages(&self, chat_id: &str) -> Result<Vec<UnifiedMessage>> {
        let client = self.client.lock().await
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Telegram client not started"))?;

        let peer = self
            .peer_cache
            .get(chat_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown chat_id: {}", chat_id))?;

        let mut iter = client.iter_messages(peer).limit(50);
        let mut messages = Vec::new();

        while let Some(msg) = iter
            .next()
            .await
            .map_err(|e| anyhow::anyhow!("iter_messages error: {}", e))?
        {
            if let Some(unified) = grammers_message_to_unified(&msg, chat_id) {
                messages.push(unified);
            }
        }

        // iter_messages returns newest-first; reverse to chronological order.
        messages.reverse();
        Ok(messages)
    }

    async fn mark_as_read(&self, chat_id: &str, _msg_ids: Vec<String>) -> Result<()> {
        let client = self.client.lock().await
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Telegram client not started"))?;

        let peer = self
            .peer_cache
            .get(chat_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown chat_id (not in peer cache): {}", chat_id))?;

        client
            .mark_as_read(peer)
            .await
            .map_err(|e| anyhow::anyhow!("mark_as_read failed: {}", e))?;
        Ok(())
    }

    fn name(&self) -> &str {
        "Telegram"
    }

    fn platform(&self) -> Platform {
        Platform::Telegram
    }

    fn auth_status(&self) -> AuthStatus {
        self.auth_status
    }
}
