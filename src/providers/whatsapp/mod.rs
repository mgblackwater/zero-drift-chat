pub mod convert;

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use whatsapp_rust::bot::Bot;
use whatsapp_rust::store::SqliteStore;
use whatsapp_rust::transport::{TokioWebSocketTransportFactory, UreqHttpClient};

use crate::core::provider::{MessagingProvider, ProviderEvent};
use crate::core::types::*;
use crate::core::Result;

use convert::*;

pub struct WhatsAppProvider {
    client: Option<Arc<whatsapp_rust::Client>>,
    bot_handle: Option<JoinHandle<()>>,
    tx: Option<mpsc::UnboundedSender<ProviderEvent>>,
    auth_status: AuthStatus,
    session_db_path: String,
    initial_lid_mappings: std::collections::HashMap<String, String>,
}

impl WhatsAppProvider {
    pub fn new(session_db_path: String) -> Self {
        Self {
            client: None,
            bot_handle: None,
            tx: None,
            auth_status: AuthStatus::NotAuthenticated,
            session_db_path,
            initial_lid_mappings: std::collections::HashMap::new(),
        }
    }

    pub fn new_with_lid_mappings(
        session_db_path: String,
        lid_mappings: std::collections::HashMap<String, String>,
    ) -> Self {
        Self {
            client: None,
            bot_handle: None,
            tx: None,
            auth_status: AuthStatus::NotAuthenticated,
            session_db_path,
            initial_lid_mappings: lid_mappings,
        }
    }
}

#[async_trait]
impl MessagingProvider for WhatsAppProvider {
    async fn start(&mut self, tx: mpsc::UnboundedSender<ProviderEvent>) -> Result<()> {
        self.auth_status = AuthStatus::Authenticating;
        let _ = tx.send(ProviderEvent::AuthStatusChanged(
            Platform::WhatsApp,
            AuthStatus::Authenticating,
        ));

        let backend = SqliteStore::new(&self.session_db_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create WhatsApp SQLite backend: {}", e))?;
        let backend = Arc::new(backend);

        tracing::info!("WhatsApp SQLite backend initialized at {}", self.session_db_path);

        let transport_factory = TokioWebSocketTransportFactory::new();
        let http_client = UreqHttpClient::new();

        let tx_events = tx.clone();
        let jid_cache = JidCache::new_with_mappings(
            std::mem::take(&mut self.initial_lid_mappings),
            tx.clone(),
        );
        let jid_cache_clone = jid_cache.clone();

        let mut bot = Bot::builder()
            .with_backend(backend)
            .with_transport_factory(transport_factory)
            .with_http_client(http_client)
            .on_event(move |event, _client| {
                let tx = tx_events.clone();
                let cache = jid_cache_clone.clone();
                async move {
                    handle_wa_event(event, &tx, &cache);
                }
            })
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to build WhatsApp bot: {}", e))?;

        let client = bot.client();
        self.client = Some(client);

        let bot_join_handle = bot
            .run()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start WhatsApp bot: {}", e))?;

        let handle = tokio::spawn(async move {
            if let Err(e) = bot_join_handle.await {
                tracing::error!("WhatsApp bot task error: {}", e);
            }
        });

        self.bot_handle = Some(handle);
        self.tx = Some(tx);
        tracing::info!("WhatsApp provider started");

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(handle) = self.bot_handle.take() {
            handle.abort();
        }
        self.client = None;
        self.auth_status = AuthStatus::NotAuthenticated;
        tracing::info!("WhatsApp provider stopped");
        Ok(())
    }

    async fn send_message(&self, chat_id: &str, content: MessageContent) -> Result<UnifiedMessage> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("WhatsApp client not connected"))?;

        let jid = chat_id_to_jid(chat_id)
            .ok_or_else(|| anyhow::anyhow!("Invalid WhatsApp chat ID: {}", chat_id))?;

        let text = match &content {
            MessageContent::Text(t) => t.clone(),
            other => other.as_text().to_string(),
        };

        let wa_msg = text_to_wa_message(&text);

        let msg_id = client
            .send_message(jid, wa_msg)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send WhatsApp message: {}", e))?;

        let unified = UnifiedMessage {
            id: msg_id,
            chat_id: chat_id.to_string(),
            platform: Platform::WhatsApp,
            sender: "You".to_string(),
            content,
            timestamp: Utc::now(),
            status: MessageStatus::Sent,
            is_outgoing: true,
        };

        if let Some(tx) = &self.tx {
            let _ = tx.send(ProviderEvent::NewMessage(unified.clone()));
        }

        Ok(unified)
    }

    async fn mark_as_read(&self, chat_id: &str, msg_ids: Vec<String>) -> Result<()> {
        if msg_ids.is_empty() {
            return Ok(());
        }
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("WhatsApp client not connected"))?;

        let jid = chat_id_to_jid(chat_id)
            .ok_or_else(|| anyhow::anyhow!("Invalid WhatsApp chat ID: {}", chat_id))?;

        client
            .mark_as_read(&jid, None, msg_ids)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send read receipt: {}", e))?;

        Ok(())
    }

    async fn get_chats(&self) -> Result<Vec<UnifiedChat>> {
        Ok(Vec::new())
    }

    async fn get_messages(&self, _chat_id: &str) -> Result<Vec<UnifiedMessage>> {
        Ok(Vec::new())
    }

    fn name(&self) -> &str {
        "WhatsApp"
    }

    fn platform(&self) -> Platform {
        Platform::WhatsApp
    }

    fn auth_status(&self) -> AuthStatus {
        self.auth_status
    }
}

/// Handle a WhatsApp event and forward it to our provider event channel.
fn handle_wa_event(
    event: whatsapp_rust::types::events::Event,
    tx: &mpsc::UnboundedSender<ProviderEvent>,
    jid_cache: &JidCache,
) {
    use whatsapp_rust::types::events::Event;
    use whatsapp_rust::Jid;

    match event {
        Event::PairingQrCode { code, timeout } => {
            tracing::info!("QR code received (valid for {}s)", timeout.as_secs());
            let _ = tx.send(ProviderEvent::AuthQrCode(code));
        }
        Event::Connected(_) => {
            tracing::info!("WhatsApp connected");
            let _ = tx.send(ProviderEvent::AuthStatusChanged(
                Platform::WhatsApp,
                AuthStatus::Authenticated,
            ));
        }
        Event::LoggedOut(_) => {
            tracing::warn!("WhatsApp logged out");
            let _ = tx.send(ProviderEvent::AuthStatusChanged(
                Platform::WhatsApp,
                AuthStatus::NotAuthenticated,
            ));
        }
        Event::PairSuccess(_) => {
            tracing::info!("WhatsApp pairing successful");
        }
        Event::PairError(_) => {
            tracing::error!("WhatsApp pairing failed");
            let _ = tx.send(ProviderEvent::AuthStatusChanged(
                Platform::WhatsApp,
                AuthStatus::Failed,
            ));
        }
        Event::Message(msg, info) => {
            let source = &info.source;

            // Record LID↔PN mapping if alternate JID is available
            if let Some(ref alt) = source.sender_alt {
                jid_cache.record_mapping(&source.sender, alt);
            }

            if let Some(unified) = wa_message_to_unified(
                &msg,
                &info.push_name,
                &info.id.to_string(),
                info.timestamp,
                &source.chat,
                &source.sender,
                source.is_from_me,
                source.is_group,
                jid_cache,
            ) {
                let chat_jid_str = source.chat.to_string();
                let kind = if chat_jid_str.ends_with("@newsletter") {
                    ChatKind::Newsletter
                } else if chat_jid_str.ends_with("@g.us") {
                    ChatKind::Group
                } else {
                    ChatKind::Chat
                };
                let chat_id = jid_to_chat_id(&source.chat, jid_cache);

                // Determine chat name:
                // - Groups/newsletters: never use sender's push_name (that's a person, not the group)
                // - 1:1 incoming: use push_name if available
                // - Outgoing / fallback: use phone number from JID
                let chat_name = if matches!(kind, ChatKind::Group | ChatKind::Newsletter)
                    || source.is_from_me
                {
                    jid_to_display_name(&source.chat)
                } else if !info.push_name.is_empty() {
                    info.push_name.clone()
                } else {
                    jid_to_display_name(&source.sender)
                };

                let preview = unified.content.as_text().to_string();
                let chat = UnifiedChat {
                    id: chat_id,
                    platform: Platform::WhatsApp,
                    name: chat_name,
                    display_name: None,
                    last_message: Some(preview),
                    unread_count: if unified.is_outgoing { 0 } else { 1 },
                    kind,
                    is_pinned: false,
                    is_muted: false,
                };

                let _ = tx.send(ProviderEvent::ChatsUpdated(vec![chat]));
                let _ = tx.send(ProviderEvent::NewMessage(unified));
            }
        }
        Event::Receipt(receipt) => {
            let type_str = format!("{:?}", receipt.r#type);
            let status = match type_str.as_str() {
                "Read" | "ReadSelf" => MessageStatus::Read,
                _ => MessageStatus::Delivered,
            };
            for msg_id in &receipt.message_ids {
                let _ = tx.send(ProviderEvent::MessageStatusUpdate {
                    message_id: msg_id.to_string(),
                    status,
                });
            }
            if type_str == "ReadSelf" {
                let chat_id = jid_to_chat_id(&receipt.source.chat, jid_cache);
                let _ = tx.send(ProviderEvent::SelfRead { chat_id });
            }
        }
        Event::JoinedGroup(lazy_conv) => {
            if let Some(conv) = lazy_conv.get() {
                let jid_str = &conv.id;
                tracing::info!(
                    "JoinedGroup (history sync): {} ({} messages)",
                    jid_str,
                    conv.messages.len()
                );

                // Record LID→PN mapping if available
                if let Some(ref lid_jid) = conv.lid_jid {
                    if lid_jid.ends_with("@lid") && !jid_str.ends_with("@lid") {
                        jid_cache.record_lid_to_pn(lid_jid, jid_str);
                    } else if jid_str.ends_with("@lid") && !lid_jid.ends_with("@lid") {
                        jid_cache.record_lid_to_pn(jid_str, lid_jid);
                    }
                }

                if let Ok(jid) = jid_str.parse::<Jid>() {
                    let chat_id = jid_to_chat_id(&jid, jid_cache);
                    let kind = if jid_str.ends_with("@newsletter") {
                        ChatKind::Newsletter
                    } else if jid_str.ends_with("@g.us") {
                        ChatKind::Group
                    } else {
                        ChatKind::Chat
                    };

                    let name = if matches!(kind, ChatKind::Group) {
                        conv.name
                            .clone()
                            .unwrap_or_else(|| jid_to_display_name(&jid))
                    } else {
                        conv.display_name
                            .clone()
                            .unwrap_or_else(|| jid_to_display_name(&jid))
                    };

                    // Convert and emit each history message
                    let mut msg_count = 0;
                    let mut last_preview = None;
                    for hsm in &conv.messages {
                        if let Some(ref web_msg) = hsm.message {
                            if let Some(unified) = web_msg_to_unified(web_msg, jid_cache) {
                                last_preview = Some(unified.content.as_text().to_string());
                                let _ = tx.send(ProviderEvent::NewMessage(unified));
                                msg_count += 1;
                            }
                        }
                    }

                    let chat = UnifiedChat {
                        id: chat_id,
                        platform: Platform::WhatsApp,
                        name,
                        display_name: None,
                        last_message: last_preview,
                        unread_count: conv.unread_count.unwrap_or(0),
                        kind,
                        is_pinned: false,
                        is_muted: false,
                    };

                    let _ = tx.send(ProviderEvent::ChatsUpdated(vec![chat]));

                    if msg_count > 0 {
                        tracing::info!(
                            "History sync: emitted {} messages for {}",
                            msg_count,
                            jid_str
                        );
                    }
                }
            }
        }
        Event::MarkChatAsReadUpdate(update) => {
            if update.action.read == Some(true) {
                let chat_id = jid_to_chat_id(&update.jid, jid_cache);
                tracing::debug!("MarkChatAsRead sync from other device: {}", chat_id);
                let _ = tx.send(ProviderEvent::SelfRead { chat_id });
            }
        }
        Event::OfflineSyncCompleted(_) => {
            tracing::info!("WhatsApp offline sync completed");
            let _ = tx.send(ProviderEvent::SyncCompleted);
        }
        _ => {
            tracing::trace!("Unhandled WhatsApp event");
        }
    }
}
