use async_trait::async_trait;
use tokio::sync::mpsc;

use super::error::Result;
use super::types::*;

#[derive(Debug, Clone)]
pub enum ProviderEvent {
    NewMessage(UnifiedMessage),
    MessageStatusUpdate {
        message_id: String,
        status: MessageStatus,
    },
    ChatsUpdated(Vec<UnifiedChat>),
    AuthStatusChanged(Platform, AuthStatus),
    AuthQrCode(String),
    SyncCompleted,
    SelfRead { chat_id: String },
    // Telegram interactive auth — Option<String> carries retry error hint
    AuthPhonePrompt(Platform, Option<String>),
    AuthOtpPrompt(Platform, Option<String>),
    AuthPasswordPrompt(Platform, Option<String>),
    /// A WhatsApp LID↔PN JID mapping was discovered at runtime.
    /// `lid` and `pn` are raw JID strings (no `wa-` prefix).
    /// The app layer should persist this and remove the stale `wa-<lid>` chat entry.
    LidPnMappingDiscovered { lid: String, pn: String },
}

#[async_trait]
#[allow(dead_code)]
pub trait MessagingProvider: Send + Sync {
    async fn start(&mut self, tx: mpsc::UnboundedSender<ProviderEvent>) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
    async fn send_message(&self, chat_id: &str, content: MessageContent) -> Result<UnifiedMessage>;
    async fn get_chats(&self) -> Result<Vec<UnifiedChat>>;
    async fn get_messages(&self, chat_id: &str) -> Result<Vec<UnifiedMessage>>;
    async fn mark_as_read(&self, _chat_id: &str, _msg_ids: Vec<String>) -> Result<()> {
        Ok(())
    }
    fn name(&self) -> &str;
    fn platform(&self) -> Platform;
    fn auth_status(&self) -> AuthStatus;
}
