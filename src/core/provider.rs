use async_trait::async_trait;
use tokio::sync::mpsc;

use super::error::Result;
use super::types::*;

/// Opaque bytes of a downloaded + decrypted media file.
pub type MediaBytes = Vec<u8>;

#[derive(Debug, Clone)]
pub enum ProviderEvent {
    NewMessage(UnifiedMessage),
    /// An existing message was edited (e.g. a bot streaming its response token-by-token).
    /// The TUI should replace the message with the same `id` in-place rather than appending.
    MessageUpdated(UnifiedMessage),
    MessageStatusUpdate {
        message_id: String,
        status: MessageStatus,
    },
    ChatsUpdated(Vec<UnifiedChat>),
    AuthStatusChanged(Platform, AuthStatus),
    AuthQrCode(String),
    SyncCompleted,
    SelfRead {
        chat_id: String,
    },
    // Telegram interactive auth — Option<String> carries retry error hint
    AuthPhonePrompt(Platform, Option<String>),
    AuthOtpPrompt(Platform, Option<String>),
    AuthPasswordPrompt(Platform, Option<String>),
    /// A WhatsApp LID↔PN JID mapping was discovered at runtime.
    /// `lid` and `pn` are raw JID strings (no `wa-` prefix).
    /// The app layer should persist this and remove the stale `wa-<lid>` chat entry.
    LidPnMappingDiscovered {
        lid: String,
        pn: String,
    },
    /// A contact in the given chat is currently typing.
    /// Fires on each typing update from the platform; the TUI expires the
    /// indicator automatically after 5 seconds without a new event.
    Typing {
        chat_id: String,
        user_name: String,
    },
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
    /// Download and decrypt a media file identified by `params`.
    ///
    /// Returns the raw plaintext bytes of the media.
    /// The default implementation returns an error; providers that serve E2EE
    /// media (e.g. WhatsApp) must override this.
    async fn download_media(&self, _params: &MediaDecryptParams) -> Result<MediaBytes> {
        Err(anyhow::anyhow!(
            "download_media not supported by this provider"
        ))
    }
    fn name(&self) -> &str;
    fn platform(&self) -> Platform;
    fn auth_status(&self) -> AuthStatus;
}
