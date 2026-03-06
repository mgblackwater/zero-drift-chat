use async_trait::async_trait;
use anyhow::Result;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct ContextMessage {
    pub role: MessageRole,
    pub content: String,
}

impl ContextMessage {
    pub fn to_chat_line(&self) -> String {
        match self.role {
            MessageRole::User => format!("[You]: {}", self.content),
            MessageRole::Assistant => format!("[Them]: {}", self.content),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub system: String,
    pub context: Vec<ContextMessage>,
    pub partial_input: String,
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn complete(&self, req: CompletionRequest, cancel: CancellationToken) -> Result<String>;
    fn clone_box(&self) -> Box<dyn AiProvider>;
}

pub mod openai;
pub mod anthropic;
pub mod gemini;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_message_display_outgoing() {
        let msg = ContextMessage { role: MessageRole::User, content: "hello".to_string() };
        assert_eq!(msg.to_chat_line(), "[You]: hello");
    }

    #[test]
    fn context_message_display_incoming() {
        let msg = ContextMessage { role: MessageRole::Assistant, content: "hi".to_string() };
        assert_eq!(msg.to_chat_line(), "[Them]: hi");
    }
}
