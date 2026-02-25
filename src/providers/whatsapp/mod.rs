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
}

impl WhatsAppProvider {
    pub fn new(session_db_path: String) -> Self {
        Self {
            client: None,
            bot_handle: None,
            tx: None,
            auth_status: AuthStatus::NotAuthenticated,
            session_db_path,
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

        let mut bot = Bot::builder()
            .with_backend(backend)
            .with_transport_factory(transport_factory)
            .with_http_client(http_client)
            .on_event(move |event, _client| {
                let tx = tx_events.clone();
                async move {
                    handle_wa_event(event, &tx);
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
fn handle_wa_event(event: whatsapp_rust::types::events::Event, tx: &mpsc::UnboundedSender<ProviderEvent>) {
    use whatsapp_rust::types::events::Event;

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
            if let Some(unified) = wa_message_to_unified(
                &msg,
                &info.push_name,
                &info.id.to_string(),
                info.timestamp,
                &source.chat,
                &source.sender,
                source.is_from_me,
                source.is_group,
            ) {
                let chat_id = jid_to_chat_id(&source.chat);

                // Determine chat name: use push_name for incoming personal chats,
                // extract phone number as fallback, skip name update for outgoing
                let chat_name = if source.is_from_me {
                    // For outgoing messages we don't know the recipient's name,
                    // so use the phone number from the chat JID as a placeholder
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
                    last_message: Some(preview),
                    unread_count: if unified.is_outgoing { 0 } else { 1 },
                    is_group: source.is_group,
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
        }
        Event::HistorySync(_) => {
            tracing::debug!("History sync event received");
        }
        Event::OfflineSyncCompleted(_) => {
            tracing::info!("WhatsApp offline sync completed");
        }
        _ => {
            tracing::trace!("Unhandled WhatsApp event");
        }
    }
}
