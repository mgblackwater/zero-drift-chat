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
}

#[async_trait]
pub trait MessagingProvider: Send + Sync {
    async fn start(&mut self, tx: mpsc::UnboundedSender<ProviderEvent>) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
    async fn send_message(&self, chat_id: &str, content: MessageContent) -> Result<UnifiedMessage>;
    async fn get_chats(&self) -> Result<Vec<UnifiedChat>>;
    async fn get_messages(&self, chat_id: &str) -> Result<Vec<UnifiedMessage>>;
    fn name(&self) -> &str;
    fn platform(&self) -> Platform;
    fn auth_status(&self) -> AuthStatus;
}
