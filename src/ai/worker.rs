use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::ai::context::{build_context, RawMessage, SYSTEM_PROMPT};
use crate::ai::providers::{AiProvider, CompletionRequest};
use crate::config::settings::AiConfig;
use crate::tui::event::AppEvent;

pub struct AiWorker {
    provider: Box<dyn AiProvider>,
    config: AiConfig,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    cancel_token: Option<CancellationToken>,
}

pub struct AiRequest {
    pub partial_input: String,
    pub messages: Vec<RawMessage>,
    pub summary: Option<String>,
}

impl AiWorker {
    pub fn new(
        provider: Box<dyn AiProvider>,
        config: AiConfig,
        event_tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Self {
        Self { provider, config, event_tx, cancel_token: None }
    }

    /// If messages exceed the threshold, asynchronously generate a summary
    /// and send the (key, value) pair to be stored in preferences DB.
    pub fn maybe_generate_summary(
        &self,
        chat_id: String,
        messages: Vec<crate::ai::context::RawMessage>,
        threshold: usize,
        db_tx: tokio::sync::mpsc::UnboundedSender<(String, String)>,
    ) {
        if messages.len() < threshold {
            return;
        }

        let provider = self.provider.clone_box();
        let model = self.config.model.clone();

        tokio::spawn(async move {
            // Summarise the older portion (everything except the last threshold/2 messages)
            let keep_recent = threshold / 2;
            let older: Vec<String> = messages.iter()
                .take(messages.len().saturating_sub(keep_recent))
                .map(|m| m.to_chat_line_owned())
                .collect();

            if older.is_empty() {
                return;
            }

            let prompt = format!(
                "Summarise this conversation in 2-3 sentences:\n\n{}",
                older.join("\n")
            );

            let req = crate::ai::providers::CompletionRequest {
                model,
                system: "You summarise conversations concisely.".to_string(),
                context: vec![],
                partial_input: prompt,
            };

            let cancel = tokio_util::sync::CancellationToken::new();
            if let Ok(summary) = provider.complete(req, cancel).await {
                if !summary.is_empty() {
                    let key = format!("ai_summary:{}", chat_id);
                    let _ = db_tx.send((key, summary));
                }
            }
        });
    }

    /// Cancel any inflight request and fire a new one.
    pub fn request(&mut self, req: AiRequest) {
        // Cancel previous inflight request
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }

        let token = CancellationToken::new();
        self.cancel_token = Some(token.clone());

        let provider = self.provider.clone_box();
        let event_tx = self.event_tx.clone();
        let model = self.config.model.clone();
        let last_n = self.config.context_messages;

        tokio::spawn(async move {
            let context = build_context(&req.messages, req.summary.as_deref(), last_n);

            let completion_req = CompletionRequest {
                model,
                system: SYSTEM_PROMPT.to_string(),
                context,
                partial_input: req.partial_input,
            };

            let check_token = token.clone();
            match provider.complete(completion_req, token).await {
                Ok(text) if !text.is_empty() => {
                    let _ = event_tx.send(AppEvent::AiSuggestion(text));
                }
                Err(_) if check_token.is_cancelled() => {
                    // Request was cancelled — silently ignore
                }
                Err(e) => {
                    let _ = event_tx.send(AppEvent::AiError(e.to_string()));
                }
                _ => {}
            }
        });
    }
}
