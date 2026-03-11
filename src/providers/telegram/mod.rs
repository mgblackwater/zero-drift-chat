pub mod convert;

use std::sync::Arc;

use async_trait::async_trait;
use grammers_client::client::UpdatesConfiguration;
use grammers_client::peer::Peer;
use grammers_client::{Client, SenderPool, SignInError};
use grammers_client::update::Update;
use grammers_session::storages::SqliteSession;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::core::error::Result;
use crate::core::provider::{MessagingProvider, ProviderEvent};
use crate::core::types::*;

use convert::{grammers_message_to_unified, peer_id_to_chat_id, PeerCache};

use grammers_client::client::PasswordToken;

/// Messages sent *into* the provider during interactive auth.
pub enum AuthInput {
    Phone(String),
    Otp(String),
    Password(String),
}

pub struct TelegramProvider {
    api_id: i32,
    api_hash: String,
    session_path: String,

    client: Option<Client>,
    peer_cache: PeerCache,
    auth_status: AuthStatus,

    /// Channel the app uses to push auth answers back into the running start() future.
    pub auth_tx: mpsc::UnboundedSender<AuthInput>,
    auth_rx: Option<mpsc::UnboundedReceiver<AuthInput>>,

    /// Background task handles (runner + update loop).
    runner_handle: Option<JoinHandle<()>>,
    update_handle: Option<JoinHandle<()>>,
}

impl TelegramProvider {
    pub fn new(api_id: i32, api_hash: String, session_path: String) -> Self {
        let (auth_tx, auth_rx) = mpsc::unbounded_channel();
        Self {
            api_id,
            api_hash,
            session_path,
            client: None,
            peer_cache: PeerCache::new(),
            auth_status: AuthStatus::NotAuthenticated,
            auth_tx,
            auth_rx: Some(auth_rx),
            runner_handle: None,
            update_handle: None,
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
        const MAX_AUTH_RETRIES: u8 = 3;
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
                        let _ = tx.send(ProviderEvent::AuthStatusChanged(Platform::Telegram, AuthStatus::Failed));
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
                        let _ = tx.send(ProviderEvent::AuthStatusChanged(Platform::Telegram, AuthStatus::Failed));
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
}

#[async_trait]
impl MessagingProvider for TelegramProvider {
    async fn start(&mut self, tx: mpsc::UnboundedSender<ProviderEvent>) -> Result<()> {
        self.auth_status = AuthStatus::Authenticating;
        let _ = tx.send(ProviderEvent::AuthStatusChanged(
            Platform::Telegram,
            AuthStatus::Authenticating,
        ));

        // Open (or create) the SQLite session file.
        let session = SqliteSession::open(&self.session_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to open Telegram session: {}", e))?;
        let session = Arc::new(session);

        // Build the sender pool.
        let pool = SenderPool::new(Arc::clone(&session), self.api_id);

        // Construct the client from the fat handle.
        let client = Client::new(pool.handle);

        // Spawn the network runner as a background task.
        let runner_handle = tokio::spawn(pool.runner.run());

        // Authenticate if needed.
        if !client
            .is_authorized()
            .await
            .map_err(|e| anyhow::anyhow!("is_authorized failed: {}", e))?
        {
            let mut auth_rx = self
                .auth_rx
                .take()
                .ok_or_else(|| anyhow::anyhow!("auth_rx already consumed"))?;

            Self::authenticate(&client, &self.api_hash, &mut auth_rx, &tx).await?;

            // Put the receiver back so a future reconnect can reuse it.
            self.auth_rx = Some(auth_rx);
        }

        self.auth_status = AuthStatus::Authenticated;
        let _ = tx.send(ProviderEvent::AuthStatusChanged(
            Platform::Telegram,
            AuthStatus::Authenticated,
        ));

        // Spawn the update-reading loop.
        let update_client = client.clone();
        let peer_cache = self.peer_cache.clone();
        let update_tx = tx.clone();
        let updates_rx = pool.updates;
        let update_handle = tokio::spawn(async move {
            let mut stream = update_client
                .stream_updates(updates_rx, UpdatesConfiguration::default())
                .await;
            let mut consecutive_errors = 0u8;
            let mut backoff_secs = 1u64;
            loop {
                match stream.next().await {
                    Ok(update) => {
                        consecutive_errors = 0;
                        backoff_secs = 1;
                        match update {
                            Update::NewMessage(msg) if !msg.outgoing() => {
                                let peer_id = msg.peer_id();
                                let chat_id_str = peer_id
                                    .bot_api_dialog_id()
                                    .map(peer_id_to_chat_id)
                                    .unwrap_or_else(|| "tg-unknown".to_string());

                                // Cache peer ref if available (async lookup from session).
                                if let Some(peer_ref) = msg.peer_ref().await {
                                    peer_cache.insert(&chat_id_str, peer_ref);
                                }

                                if let Some(unified) = grammers_message_to_unified(&msg, &chat_id_str)
                                {
                                    let _ = update_tx.send(ProviderEvent::NewMessage(unified));
                                }
                            }
                            Update::MessageEdited(msg) => {
                                // Re-emit edited messages as new (v1 simplification)
                                let peer_id = msg.peer_id();
                                let chat_id_str = peer_id
                                    .bot_api_dialog_id()
                                    .map(peer_id_to_chat_id)
                                    .unwrap_or_else(|| "tg-unknown".to_string());
                                if let Some(unified) = grammers_message_to_unified(&msg, &chat_id_str) {
                                    let _ = update_tx.send(ProviderEvent::NewMessage(unified));
                                }
                            }
                            _ => {
                                tracing::trace!("Unhandled Telegram update");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Telegram update stream error: {}", e);
                        consecutive_errors += 1;
                        if consecutive_errors >= 3 {
                            let _ = update_tx.send(ProviderEvent::AuthStatusChanged(Platform::Telegram, AuthStatus::Failed));
                            tracing::error!("Telegram: 3 consecutive update errors, stopping");
                            break;
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
                        backoff_secs = (backoff_secs * 2).min(4);
                    }
                }
            }
        });

        self.client = Some(client);
        self.runner_handle = Some(runner_handle);
        self.update_handle = Some(update_handle);

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(h) = self.update_handle.take() {
            h.abort();
        }
        if let Some(h) = self.runner_handle.take() {
            h.abort();
        }
        if let Some(client) = &self.client {
            client.disconnect();
        }
        self.client = None;
        self.auth_status = AuthStatus::NotAuthenticated;
        Ok(())
    }

    async fn send_message(&self, chat_id: &str, content: MessageContent) -> Result<UnifiedMessage> {
        let client = self
            .client
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
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Telegram client not started"))?;

        let mut dialogs = client.iter_dialogs();
        let mut chats = Vec::new();
        const MAX_DIALOGS: usize = 200;
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

            let is_group = matches!(peer, Peer::Group(_));

            chats.push(UnifiedChat {
                id: chat_id_str,
                platform: Platform::Telegram,
                name,
                display_name: None,
                last_message,
                unread_count: 0,
                is_group,
                is_pinned: false,
                is_newsletter: false,
                is_muted: false,
            });
        }

        Ok(chats)
    }

    async fn get_messages(&self, chat_id: &str) -> Result<Vec<UnifiedMessage>> {
        let client = self
            .client
            .as_ref()
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
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Telegram client not started"))?;

        if let Some(peer) = self.peer_cache.get(chat_id) {
            client
                .mark_as_read(peer)
                .await
                .map_err(|e| anyhow::anyhow!("mark_as_read failed: {}", e))?;
        }
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
