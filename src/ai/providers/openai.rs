use async_trait::async_trait;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{AiProvider, CompletionRequest, MessageRole};

#[derive(Clone)]
pub struct OpenAiClient {
    pub base_url: String,
    pub api_key: Option<String>,
    client: reqwest::Client,
}

impl OpenAiClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        Self {
            base_url,
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    stream: bool,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: String,
}

#[async_trait]
impl AiProvider for OpenAiClient {
    async fn complete(&self, req: CompletionRequest, cancel: CancellationToken) -> Result<String> {
        let mut messages = vec![
            ChatMessage { role: "system".to_string(), content: req.system.clone() },
        ];

        for ctx_msg in &req.context {
            let role = match ctx_msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
            };
            messages.push(ChatMessage {
                role: role.to_string(),
                content: ctx_msg.content.clone(),
            });
        }

        messages.push(ChatMessage {
            role: "user".to_string(),
            content: format!("Complete this message (reply with ONLY the completion, nothing else): {}", req.partial_input),
        });

        let body = ChatRequest {
            model: req.model.clone(),
            messages,
            max_tokens: 80,
            stream: false,
        };

        let mut builder = self.client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body);

        if let Some(key) = &self.api_key {
            builder = builder.bearer_auth(key);
        }

        tokio::select! {
            _ = cancel.cancelled() => Err(anyhow!("cancelled")),
            res = builder.send() => {
                let resp: ChatResponse = res?.error_for_status()?.json().await?;
                Ok(resp.choices.into_iter().next()
                    .map(|c| c.message.content.trim().to_string())
                    .unwrap_or_default())
            }
        }
    }

    fn clone_box(&self) -> Box<dyn AiProvider> {
        Box::new(self.clone())
    }
}
