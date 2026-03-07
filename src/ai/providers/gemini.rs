use async_trait::async_trait;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{AiProvider, CompletionRequest, MessageRole};

#[derive(Clone)]
pub struct GeminiClient {
    pub api_key: Option<String>,
    client: reqwest::Client,
}

impl GeminiClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key, client: reqwest::Client::new() }
    }
}

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "systemInstruction")]
    system_instruction: GeminiSystemInstruction,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiGenerationConfig,
}

#[derive(Serialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiGenerationConfig {
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiCandidateContent,
}

#[derive(Deserialize)]
struct GeminiCandidateContent {
    parts: Vec<GeminiResponsePart>,
}

#[derive(Deserialize)]
struct GeminiResponsePart {
    text: String,
}

#[async_trait]
impl AiProvider for GeminiClient {
    async fn complete(&self, req: CompletionRequest, cancel: CancellationToken) -> Result<String> {
        let api_key = self.api_key.as_deref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| anyhow!("Gemini API key not configured"))?;

        let mut contents: Vec<GeminiContent> = req.context.iter().map(|m| {
            let role = match m.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "model",
            };
            GeminiContent {
                role: role.to_string(),
                parts: vec![GeminiPart { text: m.content.clone() }],
            }
        }).collect();

        contents.push(GeminiContent {
            role: "user".to_string(),
            parts: vec![GeminiPart {
                text: format!("Complete this message (reply with ONLY the completion): {}", req.partial_input),
            }],
        });

        let body = GeminiRequest {
            system_instruction: GeminiSystemInstruction {
                parts: vec![GeminiPart { text: req.system.clone() }],
            },
            contents,
            generation_config: GeminiGenerationConfig { max_output_tokens: 80 },
        };

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            req.model, api_key
        );

        let request = self.client.post(&url).json(&body);

        tokio::select! {
            _ = cancel.cancelled() => Err(anyhow!("cancelled")),
            res = request.send() => {
                let resp: GeminiResponse = res?.error_for_status()?.json().await?;
                Ok(resp.candidates.into_iter().next()
                    .and_then(|c| c.content.parts.into_iter().next())
                    .map(|p| p.text.trim().to_string())
                    .unwrap_or_default())
            }
        }
    }

    fn clone_box(&self) -> Box<dyn AiProvider> {
        Box::new(self.clone())
    }
}
