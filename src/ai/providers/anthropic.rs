use async_trait::async_trait;
use anyhow::Result;
use tokio_util::sync::CancellationToken;

use super::{AiProvider, CompletionRequest};

#[derive(Clone)]
pub struct AnthropicClient {
    pub api_key: Option<String>,
    client: reqwest::Client,
}

impl AnthropicClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl AiProvider for AnthropicClient {
    async fn complete(&self, _req: CompletionRequest, _cancel: CancellationToken) -> Result<String> {
        anyhow::bail!("Anthropic provider not yet implemented")
    }

    fn clone_box(&self) -> Box<dyn AiProvider> {
        Box::new(self.clone())
    }
}
