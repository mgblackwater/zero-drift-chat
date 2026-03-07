# AI Autocomplete Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add ghost-text autocomplete to the chat input bar, powered by a configurable AI backend (local Ollama/llama.cpp or cloud OpenAI/Anthropic/Gemini).

**Architecture:** An `AiWorker` tokio task holds a `CancellationToken` and a `Box<dyn AiProvider>`. It receives `AiRequest` events, cancels any inflight request, and spawns a new suggestion task that sends `AppEvent::AiSuggestion` back into the main event loop. Context is assembled from a stored per-chat summary plus the last N raw messages.

**Tech Stack:** Rust async/await, tokio, tokio-util (CancellationToken), reqwest (HTTP), ratatui, existing rusqlite preferences table.

---

### Task 1: Add dependencies and `[ai]` config section

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/config/settings.rs`
- Modify: `configs/default.toml`

**Step 1: Add crate dependencies**

In `Cargo.toml`, add after the existing deps:

```toml
reqwest = { version = "0.12", features = ["json"] }
tokio-util = { version = "0.7", features = ["rt"] }
```

(`async-trait` is already present.)

**Step 2: Add `AiConfig` struct to `src/config/settings.rs`**

Add the struct and defaults:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ai_provider")]
    pub provider: String,
    #[serde(default = "default_ai_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_ai_model")]
    pub model: String,
    #[serde(default = "default_context_messages")]
    pub context_messages: usize,
    #[serde(default = "default_summary_threshold")]
    pub summary_threshold: usize,
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
}

fn default_ai_provider() -> String { "ollama".to_string() }
fn default_ai_base_url() -> String { "http://localhost:11434".to_string() }
fn default_ai_model() -> String { "qwen2.5:1.5b-instruct".to_string() }
fn default_context_messages() -> usize { 10 }
fn default_summary_threshold() -> usize { 50 }
fn default_debounce_ms() -> u64 { 500 }

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_ai_provider(),
            base_url: default_ai_base_url(),
            api_key: String::new(),
            model: default_ai_model(),
            context_messages: default_context_messages(),
            summary_threshold: default_summary_threshold(),
            debounce_ms: default_debounce_ms(),
        }
    }
}
```

Add `pub ai: AiConfig` field to `AppConfig` with `#[serde(default)]`. Add it to both `AppConfig::default()` and the struct definition.

**Step 3: Update `configs/default.toml` with commented AI section**

```toml
[ai]
# enabled = false
# provider = "ollama"          # ollama | openai | anthropic | gemini
# base_url = "http://localhost:11434"
# api_key = ""
# model = "qwen2.5:1.5b-instruct"
# context_messages = 10
# summary_threshold = 50
# debounce_ms = 500
```

**Step 4: Verify it compiles**

```bash
cargo check
```
Expected: no errors.

**Step 5: Commit**

```bash
git add Cargo.toml src/config/settings.rs configs/default.toml
git commit -m "feat: add AiConfig and reqwest/tokio-util dependencies"
```

---

### Task 2: Provider trait and OpenAI-compatible client

**Files:**
- Create: `src/ai/mod.rs`
- Create: `src/ai/providers/mod.rs`
- Create: `src/ai/providers/openai.rs`

**Step 1: Write unit test for context message formatting**

In `src/ai/providers/mod.rs`, add a test at the bottom:

```rust
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
```

**Step 2: Run test to verify it fails**

```bash
cargo test ai::providers
```
Expected: compile error — module doesn't exist yet.

**Step 3: Create `src/ai/providers/mod.rs`**

```rust
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
```

**Step 4: Create `src/ai/providers/openai.rs`**

```rust
use async_trait::async_trait;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{AiProvider, CompletionRequest};

pub struct OpenAiClient {
    pub base_url: String,
    pub api_key: String,
    client: reqwest::Client,
}

impl OpenAiClient {
    pub fn new(base_url: String, api_key: String) -> Self {
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
            messages.push(ChatMessage {
                role: "user".to_string(),
                content: ctx_msg.to_chat_line(),
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

        let mut request = self.client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body);

        if !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
        }

        tokio::select! {
            _ = cancel.cancelled() => Err(anyhow!("cancelled")),
            res = request.send() => {
                let resp: ChatResponse = res?.json().await?;
                Ok(resp.choices.into_iter().next()
                    .map(|c| c.message.content.trim().to_string())
                    .unwrap_or_default())
            }
        }
    }
}
```

**Step 5: Create stub files for anthropic and gemini (implement in Task 3)**

`src/ai/providers/anthropic.rs`:
```rust
// Stub — implemented in Task 3
use async_trait::async_trait;
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use super::{AiProvider, CompletionRequest};

pub struct AnthropicClient {
    pub api_key: String,
    client: reqwest::Client,
}

impl AnthropicClient {
    pub fn new(api_key: String) -> Self {
        Self { api_key, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl AiProvider for AnthropicClient {
    async fn complete(&self, _req: CompletionRequest, _cancel: CancellationToken) -> Result<String> {
        anyhow::bail!("Anthropic provider not yet implemented")
    }
}
```

`src/ai/providers/gemini.rs`:
```rust
// Stub — implemented in Task 3
use async_trait::async_trait;
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use super::{AiProvider, CompletionRequest};

pub struct GeminiClient {
    pub api_key: String,
    client: reqwest::Client,
}

impl GeminiClient {
    pub fn new(api_key: String) -> Self {
        Self { api_key, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl AiProvider for GeminiClient {
    async fn complete(&self, _req: CompletionRequest, _cancel: CancellationToken) -> Result<String> {
        anyhow::bail!("Gemini provider not yet implemented")
    }
}
```

**Step 6: Create `src/ai/mod.rs`** (minimal, worker added in Task 5)

```rust
pub mod providers;
pub mod context;
pub mod worker;
```

**Step 7: Create stub `src/ai/context.rs`** (implemented in Task 4)

```rust
// Stub — implemented in Task 4
```

**Step 8: Create stub `src/ai/worker.rs`** (implemented in Task 5)

```rust
// Stub — implemented in Task 5
```

**Step 9: Register module in `src/main.rs`**

Add `mod ai;` after the existing `mod` declarations in `src/main.rs`.

**Step 10: Run tests**

```bash
cargo test ai::providers::tests
```
Expected: 2 tests pass.

**Step 11: Commit**

```bash
git add src/ai/ src/main.rs
git commit -m "feat: add AI provider trait and OpenAI-compatible client"
```

---

### Task 3: Anthropic and Gemini provider implementations

**Files:**
- Modify: `src/ai/providers/anthropic.rs`
- Modify: `src/ai/providers/gemini.rs`

**Step 1: Implement Anthropic client**

Replace the stub in `src/ai/providers/anthropic.rs`:

```rust
use async_trait::async_trait;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{AiProvider, CompletionRequest};

pub struct AnthropicClient {
    pub api_key: String,
    client: reqwest::Client,
}

impl AnthropicClient {
    pub fn new(api_key: String) -> Self {
        Self { api_key, client: reqwest::Client::new() }
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
        let mut messages: Vec<AnthropicMessage> = req.context.iter().map(|m| AnthropicMessage {
            role: "user".to_string(),
            content: m.to_chat_line(),
        }).collect();

        messages.push(AnthropicMessage {
            role: "user".to_string(),
            content: format!("Complete this message (reply with ONLY the completion): {}", req.partial_input),
        });

        let body = AnthropicRequest {
            model: req.model.clone(),
            max_tokens: 80,
            system: req.system.clone(),
            messages,
        };

        let request = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body);

        tokio::select! {
            _ = cancel.cancelled() => Err(anyhow!("cancelled")),
            res = request.send() => {
                let resp: AnthropicResponse = res?.json().await?;
                Ok(resp.content.into_iter().next()
                    .map(|c| c.text.trim().to_string())
                    .unwrap_or_default())
            }
        }
    }
}
```

**Step 2: Implement Gemini client**

Replace the stub in `src/ai/providers/gemini.rs`:

```rust
use async_trait::async_trait;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{AiProvider, CompletionRequest};

pub struct GeminiClient {
    pub api_key: String,
    client: reqwest::Client,
}

impl GeminiClient {
    pub fn new(api_key: String) -> Self {
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
        let mut text_parts: Vec<String> = req.context.iter()
            .map(|m| m.to_chat_line())
            .collect();
        text_parts.push(format!("Complete this message (reply with ONLY the completion): {}", req.partial_input));

        let contents = vec![GeminiContent {
            parts: text_parts.into_iter().map(|t| GeminiPart { text: t }).collect(),
        }];

        let body = GeminiRequest {
            system_instruction: GeminiSystemInstruction {
                parts: vec![GeminiPart { text: req.system.clone() }],
            },
            contents,
            generation_config: GeminiGenerationConfig { max_output_tokens: 80 },
        };

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            req.model, self.api_key
        );

        let request = self.client.post(&url).json(&body);

        tokio::select! {
            _ = cancel.cancelled() => Err(anyhow!("cancelled")),
            res = request.send() => {
                let resp: GeminiResponse = res?.json().await?;
                Ok(resp.candidates.into_iter().next()
                    .and_then(|c| c.content.parts.into_iter().next())
                    .map(|p| p.text.trim().to_string())
                    .unwrap_or_default())
            }
        }
    }
}
```

**Step 3: Verify compilation**

```bash
cargo check
```
Expected: no errors.

**Step 4: Commit**

```bash
git add src/ai/providers/anthropic.rs src/ai/providers/gemini.rs
git commit -m "feat: add Anthropic and Gemini provider implementations"
```

---

### Task 4: Context builder

**Files:**
- Modify: `src/ai/context.rs`

**Step 1: Write unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::providers::MessageRole;

    #[test]
    fn builds_context_from_messages() {
        let messages = vec![
            RawMessage { is_outgoing: true,  text: "hey".to_string() },
            RawMessage { is_outgoing: false, text: "yo".to_string() },
        ];
        let ctx = build_context(&messages, None, 10);
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].to_chat_line(), "[You]: hey");
        assert_eq!(ctx[1].to_chat_line(), "[Them]: yo");
    }

    #[test]
    fn limits_to_last_n() {
        let messages: Vec<RawMessage> = (0..20).map(|i| RawMessage {
            is_outgoing: i % 2 == 0,
            text: format!("msg {}", i),
        }).collect();
        let ctx = build_context(&messages, None, 5);
        assert_eq!(ctx.len(), 5);
        assert_eq!(ctx.last().unwrap().to_chat_line(), "[Them]: msg 19");
    }

    #[test]
    fn prepends_summary_as_first_message() {
        let messages = vec![
            RawMessage { is_outgoing: true, text: "hi".to_string() },
        ];
        let ctx = build_context(&messages, Some("Chat about cats"), 10);
        assert_eq!(ctx.len(), 2);
        assert!(matches!(ctx[0].role, MessageRole::Assistant));
        assert!(ctx[0].content.contains("cats"));
    }
}
```

**Step 2: Run to confirm failure**

```bash
cargo test ai::context
```
Expected: compile error.

**Step 3: Implement `src/ai/context.rs`**

```rust
use crate::ai::providers::{ContextMessage, MessageRole};

pub const SYSTEM_PROMPT: &str =
    "You are a chat autocomplete assistant. Complete the user's message naturally \
     based on the conversation context. Reply with ONLY the completion text — \
     no explanation, no quotes, no prefix.";

pub struct RawMessage {
    pub is_outgoing: bool,
    pub text: String,
}

/// Assemble context from optional summary + last N messages.
pub fn build_context(
    messages: &[RawMessage],
    summary: Option<&str>,
    last_n: usize,
) -> Vec<ContextMessage> {
    let mut ctx: Vec<ContextMessage> = Vec::new();

    if let Some(s) = summary {
        ctx.push(ContextMessage {
            role: MessageRole::Assistant,
            content: format!("[Conversation summary]: {}", s),
        });
    }

    let start = messages.len().saturating_sub(last_n);
    for msg in &messages[start..] {
        ctx.push(ContextMessage {
            role: if msg.is_outgoing { MessageRole::User } else { MessageRole::Assistant },
            content: msg.text.clone(),
        });
    }

    ctx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::providers::MessageRole;

    #[test]
    fn builds_context_from_messages() {
        let messages = vec![
            RawMessage { is_outgoing: true,  text: "hey".to_string() },
            RawMessage { is_outgoing: false, text: "yo".to_string() },
        ];
        let ctx = build_context(&messages, None, 10);
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].to_chat_line(), "[You]: hey");
        assert_eq!(ctx[1].to_chat_line(), "[Them]: yo");
    }

    #[test]
    fn limits_to_last_n() {
        let messages: Vec<RawMessage> = (0..20).map(|i| RawMessage {
            is_outgoing: i % 2 == 0,
            text: format!("msg {}", i),
        }).collect();
        let ctx = build_context(&messages, None, 5);
        assert_eq!(ctx.len(), 5);
        assert_eq!(ctx.last().unwrap().to_chat_line(), "[Them]: msg 19");
    }

    #[test]
    fn prepends_summary_as_first_message() {
        let messages = vec![
            RawMessage { is_outgoing: true, text: "hi".to_string() },
        ];
        let ctx = build_context(&messages, Some("Chat about cats"), 10);
        assert_eq!(ctx.len(), 2);
        assert!(matches!(ctx[0].role, MessageRole::Assistant));
        assert!(ctx[0].content.contains("cats"));
    }
}
```

**Step 4: Run tests**

```bash
cargo test ai::context
```
Expected: 3 tests pass.

**Step 5: Commit**

```bash
git add src/ai/context.rs
git commit -m "feat: add AI context builder with summary support"
```

---

### Task 5: AI Worker

**Files:**
- Modify: `src/ai/worker.rs`
- Modify: `src/tui/event.rs`

**Step 1: Add new `AppEvent` variants to `src/tui/event.rs`**

In the `AppEvent` enum, add:

```rust
AiSuggestion(String),
AiError(String),
```

Also expose a way to clone the sender so the worker can inject events. Add a field and method to `EventHandler`:

```rust
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<AppEvent>,
    pub tx: mpsc::UnboundedSender<AppEvent>,   // add this
    _task: tokio::task::JoinHandle<()>,
}
```

And expose it:
```rust
pub fn sender(&self) -> mpsc::UnboundedSender<AppEvent> {
    self.tx.clone()
}
```

In `EventHandler::new`, save `tx` before moving it into the spawned task — clone it first:

```rust
let (tx, rx) = mpsc::unbounded_channel::<AppEvent>();
let task_tx = tx.clone();  // move task_tx into the spawn, keep tx on self
// ... use task_tx inside the spawned task instead of tx
Self { rx, tx, _task: task }
```

**Step 2: Implement `src/ai/worker.rs`**

```rust
use std::time::Duration;

use anyhow::Result;
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
        // Cancel previous
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
```

**Step 3: Add `clone_box` to the `AiProvider` trait**

In `src/ai/providers/mod.rs`, update the trait:

```rust
pub trait AiProvider: Send + Sync {
    async fn complete(&self, req: CompletionRequest, cancel: CancellationToken) -> Result<String>;
    fn clone_box(&self) -> Box<dyn AiProvider>;
}
```

Implement `clone_box` on each provider by deriving `Clone` on the client structs and returning `Box::new(self.clone())`.

For `OpenAiClient`, `AnthropicClient`, `GeminiClient` — add `#[derive(Clone)]` and:

```rust
fn clone_box(&self) -> Box<dyn AiProvider> {
    Box::new(self.clone())
}
```

Note: `reqwest::Client` is already `Clone` (it's an `Arc` internally).

**Step 4: Verify compilation**

```bash
cargo check
```
Expected: no errors.

**Step 5: Commit**

```bash
git add src/ai/worker.rs src/tui/event.rs src/ai/providers/
git commit -m "feat: add AiWorker with cancellation token and AppEvent AI variants"
```

---

### Task 6: Keybindings — Tab accept and Ctrl+Space trigger

**Files:**
- Modify: `src/tui/keybindings.rs`
- Modify: `src/tui/app_state.rs`

**Step 1: Add new `Action` variants to `src/tui/keybindings.rs`**

In the `Action` enum, add:

```rust
AiSuggestAccept,   // Tab / → at end of input when suggestion present
AiSuggestRequest,  // Ctrl+Space — on-demand trigger
```

**Step 2: Update `map_editing_mode` in `src/tui/keybindings.rs`**

Add these arms before the catch-all `_ => Action::InputKey(key)`:

```rust
(KeyCode::Tab, _) => Action::AiSuggestAccept,
(KeyCode::Char(' '), m) if m.contains(KeyModifiers::CONTROL) => Action::AiSuggestRequest,
```

**Step 3: Add `ai_suggestion` to `AppState`**

In `src/tui/app_state.rs`, add to `AppState` struct:

```rust
pub ai_suggestion: Option<String>,
pub ai_status: Option<String>,   // brief error message for status bar
```

Initialise both to `None` in `AppState::new()`.

**Step 4: Verify compilation**

```bash
cargo check
```
Expected: no errors.

**Step 5: Commit**

```bash
git add src/tui/keybindings.rs src/tui/app_state.rs
git commit -m "feat: add AiSuggestAccept/Request actions and ai_suggestion state"
```

---

### Task 7: Wire everything into `app.rs`

**Files:**
- Modify: `src/app.rs`

**Step 1: Import new types**

Add to imports in `src/app.rs`:

```rust
use crate::ai::worker::{AiWorker, AiRequest};
use crate::ai::context::RawMessage;
use crate::ai::providers::openai::OpenAiClient;
use crate::ai::providers::anthropic::AnthropicClient;
use crate::ai::providers::gemini::GeminiClient;
use std::time::{Duration, Instant};
```

**Step 2: Add `AiWorker` and debounce fields to `App`**

```rust
pub struct App {
    state: AppState,
    router: MessageRouter,
    db: Database,
    address_book: AddressBook,
    config: AppConfig,
    config_path: PathBuf,
    ai_worker: Option<AiWorker>,           // None when AI disabled
    last_keystroke: Option<Instant>,        // for debounce tracking
}
```

**Step 3: Initialise `AiWorker` in `App::new()`**

After setting up `state`, create the worker if AI is enabled. The worker needs the event sender — pass it in:

```rust
pub fn new(
    config: AppConfig,
    db: Database,
    address_book: AddressBook,
    config_path: PathBuf,
    event_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
) -> Self {
    let ai_worker = if config.ai.enabled {
        let provider: Box<dyn crate::ai::providers::AiProvider> = match config.ai.provider.as_str() {
            "anthropic" => Box::new(AnthropicClient::new(config.ai.api_key.clone())),
            "gemini"    => Box::new(GeminiClient::new(config.ai.api_key.clone())),
            _           => Box::new(OpenAiClient::new(config.ai.base_url.clone(), config.ai.api_key.clone())),
        };
        Some(AiWorker::new(provider, config.ai.clone(), event_tx))
    } else {
        None
    };

    Self {
        state: AppState::new(),
        router: MessageRouter::new(),
        db,
        address_book,
        config,
        config_path,
        ai_worker,
        last_keystroke: None,
    }
}
```

**Step 4: Update `App::new()` call site in `src/main.rs`**

In `main.rs`, after creating `EventHandler`, pass the sender to `App::new`:

```rust
let event_handler = EventHandler::new(config.tui.tick_rate_ms, config.tui.render_rate_ms);
let app = App::new(config, db, address_book, config_path, event_handler.sender());
```

**Step 5: Handle debounce on `InputKey` events**

In the `Action::InputKey` arm of the event handler loop in `app.rs`, after forwarding the key to textarea, add:

```rust
// Dismiss any existing suggestion on new keystroke
self.state.ai_suggestion = None;
// Record keystroke time for debounce
self.last_keystroke = Some(Instant::now());
```

**Step 6: Handle debounce firing on `AppEvent::Tick`**

In the `AppEvent::Tick` arm, add:

```rust
if let (Some(t), Some(worker)) = (self.last_keystroke, self.ai_worker.as_mut()) {
    let debounce = Duration::from_millis(self.config.ai.debounce_ms);
    if t.elapsed() >= debounce {
        self.last_keystroke = None;
        if self.state.input_mode == InputMode::Editing {
            let partial = self.state.input.lines().join("\n");
            if !partial.is_empty() {
                self.fire_ai_request(worker, partial);
            }
        }
    }
}
```

**Step 7: Add `fire_ai_request` helper method**

```rust
fn fire_ai_request(&self, worker: &mut AiWorker, partial: String) {
    let messages = self.state.messages.iter()
        .map(|m| {
            let text = match &m.content {
                crate::core::types::MessageContent::Text(t) => t.clone(),
                _ => String::new(),
            };
            RawMessage { is_outgoing: m.is_outgoing, text }
        })
        .filter(|m| !m.text.is_empty())
        .collect();

    let summary = self.state.selected_chat_id()
        .and_then(|id| self.db.get_preference(&format!("ai_summary:{}", id)).ok().flatten());

    worker.request(AiRequest { partial_input: partial, messages, summary });
}
```

**Step 8: Handle AI events in the main match**

```rust
AppEvent::AiSuggestion(text) => {
    if self.state.input_mode == InputMode::Editing {
        self.state.ai_suggestion = Some(text);
    }
}
AppEvent::AiError(e) => {
    self.state.ai_status = Some(format!("AI: {}", e));
}
```

**Step 9: Handle `Action::AiSuggestAccept`**

```rust
Action::AiSuggestAccept => {
    if let Some(suggestion) = self.state.ai_suggestion.take() {
        // Append suggestion text to the textarea
        for ch in suggestion.chars() {
            self.state.input.insert_char(ch);
        }
    } else {
        // No suggestion — forward Tab as literal tab or ignore
    }
}
```

**Step 10: Handle `Action::AiSuggestRequest`**

```rust
Action::AiSuggestRequest => {
    if self.state.input_mode == InputMode::Editing {
        let partial = self.state.input.lines().join("\n");
        if !partial.is_empty() {
            if let Some(worker) = self.ai_worker.as_mut() {
                self.fire_ai_request(worker, partial);
            }
        }
    }
}
```

**Step 11: Clear suggestion on mode exit**

In the `Action::ExitEditing` arm, add:

```rust
self.state.ai_suggestion = None;
```

**Step 12: Verify compilation**

```bash
cargo check
```
Expected: no errors.

**Step 13: Commit**

```bash
git add src/app.rs src/main.rs
git commit -m "feat: wire AiWorker into app event loop with debounce and key actions"
```

---

### Task 8: Ghost text rendering in input bar

**Files:**
- Modify: `src/tui/widgets/input_bar.rs`
- Modify: `src/tui/render.rs`

**Step 1: Update `render_input_bar` signature**

In `src/tui/widgets/input_bar.rs`, change the function signature to accept the suggestion:

```rust
pub fn render_input_bar(
    f: &mut Frame,
    area: Rect,
    textarea: &TextArea<'static>,
    mode: InputMode,
    ai_suggestion: Option<&str>,
) {
```

**Step 2: Render ghost text**

Inside the `if mode == InputMode::Editing` branch, after rendering the textarea widget, add:

```rust
// Render ghost text if suggestion present
if let Some(suggestion) = ai_suggestion {
    // Position ghost text after the cursor on the last line
    let lines = textarea.lines();
    let last_line = lines.last().map(|l| l.len()).unwrap_or(0);

    // Ghost text appears on a line below the input if input takes full width,
    // or we show it as a styled paragraph overlaid in the hint area.
    // Simple approach: show hint line at bottom of inner_area.
    let hint_area = Rect {
        x: inner_area.x,
        y: inner_area.y + inner_area.height.saturating_sub(1),
        width: inner_area.width,
        height: 1,
    };
    let hint_text = format!("  ↳ {}", suggestion);
    f.render_widget(
        Paragraph::new(hint_text)
            .style(Style::default().fg(Color::DarkGray)),
        hint_area,
    );
}
```

Note: This renders the ghost text as a greyed hint line at the bottom of the input area. A full inline ghost-text (same line as cursor) requires custom rendering beyond what `tui-textarea` exposes; the hint-line approach is clean and unambiguous for a TUI.

**Step 3: Update call site in `src/tui/render.rs`**

Find the call to `render_input_bar` and add `state.ai_suggestion.as_deref()` as the last argument.

**Step 4: Verify compilation**

```bash
cargo check
```
Expected: no errors.

**Step 5: Manual smoke test**

```bash
cargo run
```

- Enable AI in `configs/default.toml` (`enabled = true`, ensure Ollama is running with `qwen2.5:1.5b-instruct`)
- Open a chat, press `i` to enter INSERT mode
- Type a partial message and wait ~500ms
- Verify a greyed hint line appears below the input
- Press `Tab` to accept — text should be appended
- Press `Ctrl+Space` to trigger on demand

**Step 6: Commit**

```bash
git add src/tui/widgets/input_bar.rs src/tui/render.rs
git commit -m "feat: render AI autocomplete ghost text hint in input bar"
```

---

### Task 9: Summary generation (background task)

**Files:**
- Modify: `src/ai/worker.rs`
- Modify: `src/app.rs`

**Step 1: Add summary generation method to `AiWorker`**

In `src/ai/worker.rs`, add:

```rust
pub fn maybe_generate_summary(
    &self,
    chat_id: String,
    messages: Vec<RawMessage>,
    threshold: usize,
    db_tx: mpsc::UnboundedSender<(String, String)>,  // (pref_key, value)
) {
    if messages.len() < threshold {
        return;
    }

    let provider = self.provider.clone_box();
    let model = self.config.model.clone();

    tokio::spawn(async move {
        // Summarise first (messages.len() - threshold/2) messages
        let older: Vec<String> = messages.iter()
            .take(messages.len() - threshold / 2)
            .map(|m| m.to_chat_line_owned())
            .collect();

        let prompt = format!(
            "Summarise this conversation in 2-3 sentences:\n\n{}",
            older.join("\n")
        );

        let req = CompletionRequest {
            model,
            system: "You summarise conversations concisely.".to_string(),
            context: vec![],
            partial_input: prompt,
        };

        let cancel = CancellationToken::new();
        if let Ok(summary) = provider.complete(req, cancel).await {
            let key = format!("ai_summary:{}", chat_id);
            let _ = db_tx.send((key, summary));
        }
    });
}
```

Add `fn to_chat_line_owned(&self) -> String` to `RawMessage`:

```rust
impl RawMessage {
    pub fn to_chat_line_owned(&self) -> String {
        if self.is_outgoing { format!("[You]: {}", self.text) }
        else { format!("[Them]: {}", self.text) }
    }
}
```

**Step 2: Wire summary saves in `app.rs`**

When a new message arrives for the active chat (in the `ProviderEvent::Message` handler), check the message count and fire `maybe_generate_summary` if threshold is crossed. Save responses from `db_tx` by adding a `db_rx` channel and polling it in the Tick handler.

This is a non-blocking background operation — failures are silent (summary is optional).

**Step 3: Verify compilation**

```bash
cargo check
```
Expected: no errors.

**Step 4: Commit**

```bash
git add src/ai/worker.rs src/app.rs
git commit -m "feat: background summary generation stored in preferences"
```

---

### Task 10: Final verification

**Step 1: Run all tests**

```bash
cargo test
```
Expected: all pass.

**Step 2: Build release binary**

```bash
cargo build --release
```
Expected: success.

**Step 3: End-to-end smoke test with Ollama**

```bash
# Ensure Ollama is running and model is pulled:
ollama pull qwen2.5:1.5b-instruct

# Enable AI in config
# Set [ai] enabled = true in configs/default.toml

./target/release/zero-drift-chat
```

Checklist:
- [ ] App starts normally when `ai.enabled = false`
- [ ] App starts and connects to Ollama when `ai.enabled = true`
- [ ] Typing in INSERT mode → ghost hint appears after debounce
- [ ] `Ctrl+Space` triggers immediately
- [ ] `Tab` accepts suggestion
- [ ] `Esc` dismisses suggestion and exits INSERT mode
- [ ] Any other key dismisses suggestion and continues editing
- [ ] AI errors show briefly (non-fatal)

**Step 4: Final commit**

```bash
git add -A
git commit -m "feat: AI autocomplete — ghost text, debounce, multi-provider support"
```
