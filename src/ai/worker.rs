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

            match provider.complete(completion_req, token).await {
                Ok(text) if !text.is_empty() => {
                    let _ = event_tx.send(AppEvent::AiSuggestion(text));
                }
                Err(e) if e.to_string() != "cancelled" => {
                    let _ = event_tx.send(AppEvent::AiError(e.to_string()));
                }
                _ => {}
            }
        });
    }
}
