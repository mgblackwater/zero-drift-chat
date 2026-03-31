use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{AiProvider, CompletionRequest, MessageRole};

#[derive(Clone)]
pub struct AnthropicClient {
    pub api_key: Option<String>,
    client: reqwest::Client,
}

impl AnthropicClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: String,
}

#[async_trait]
impl AiProvider for AnthropicClient {
    async fn complete(&self, req: CompletionRequest, cancel: CancellationToken) -> Result<String> {
        let api_key = self
            .api_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| anyhow!("Anthropic API key not configured"))?;

        let mut messages: Vec<AnthropicMessage> = req
            .context
            .iter()
            .map(|m| {
                let role = match m.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                };
                AnthropicMessage {
                    role: role.to_string(),
                    content: m.content.clone(),
                }
            })
            .collect();

        messages.push(AnthropicMessage {
            role: "user".to_string(),
            content: format!(
                "Complete this message (reply with ONLY the completion): {}",
                req.partial_input
            ),
        });

        let body = AnthropicRequest {
            model: req.model.clone(),
            max_tokens: 80,
            system: req.system.clone(),
            messages,
        };

        let request = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body);

        tokio::select! {
            _ = cancel.cancelled() => Err(anyhow!("cancelled")),
            res = request.send() => {
                let resp: AnthropicResponse = res?.error_for_status()?.json().await?;
                Ok(resp.content.into_iter().next()
                    .map(|c| c.text.trim().to_string())
                    .unwrap_or_default())
            }
        }
    }

    fn clone_box(&self) -> Box<dyn AiProvider> {
        Box::new(self.clone())
    }
}
