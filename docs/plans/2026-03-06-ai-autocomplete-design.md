# AI Autocomplete Design

Date: 2026-03-06

## Overview

Add context-aware ghost-text autocomplete to the chat input bar. When typing, the AI suggests a completion inline (greyed out after the cursor). Accepted with `Tab` or `→`, dismissed with `Esc` or any other key.

Supports local models (Ollama, llama.cpp) and cloud providers (OpenAI-compatible, Anthropic, Gemini). Optimised for Qwen2.5-1.5B-Instruct running locally — zero cost, low latency.

## Architecture

```
App (event loop)
├── AiWorker (tokio task)
│   ├── debounce timer (500ms, resets on keystroke)
│   ├── CancellationToken (cancels inflight request on new trigger)
│   └── Box<dyn AiProvider>
│       ├── OpenAiClient   (covers Ollama, llama.cpp, OpenAI, Groq, Mistral)
│       ├── AnthropicClient
│       └── GeminiClient
└── ContextBuilder
    ├── Conversation summary (stored in SQLite per chat_id)
    └── Last N raw messages verbatim
```

### New modules

- `src/ai/mod.rs` — AiWorker, request/response types, cancellation
- `src/ai/context.rs` — hybrid context builder
- `src/ai/providers/mod.rs` — AiProvider trait
- `src/ai/providers/openai.rs` — OpenAI-compatible client
- `src/ai/providers/anthropic.rs` — Anthropic Messages API client
- `src/ai/providers/gemini.rs` — Gemini generateContent API client

## Trigger Behaviour

- **Debounced:** fires 500ms after the user stops typing in INSERT mode
- **On-demand:** `Ctrl+Space` fires immediately, bypassing the debounce
- Only triggers when there is partial text in the input bar
- Previous inflight request is cancelled when a new one fires

## Context Builder

### Prompt structure

```
[system]
You are a chat autocomplete assistant. Complete the user's message naturally
based on conversation context. Reply with ONLY the completion text, no explanation.

[conversation summary — optional, ~150 tokens]
Stored per chat_id in SQLite. Regenerated every 50 messages (configurable).

[last N messages — default 10]
[You]: ...
[Them]: ...

[user]
Complete this: <partial input>
```

### Summary lifecycle

- After every `summary_threshold` new messages, a background task summarises
  older messages and stores the result in the `preferences` table (key: `ai_summary:<chat_id>`)
- Summary generation never blocks autocomplete — it runs async in the background

### Token budget (Qwen2.5-1.5B-Instruct)

| Part             | Approx tokens |
|------------------|--------------|
| System prompt    | ~50          |
| Summary          | ~150         |
| Last 10 messages | ~300–500     |
| Partial input    | ~20          |
| **Total**        | **~520–720** |

## AI Worker

```rust
struct AiWorker {
    provider: Box<dyn AiProvider>,
    debounce: Duration,          // default 500ms
    cancel_token: Option<CancellationToken>,
}
```

### New AppEvent variants

```rust
AiRequest { chat_id: String, partial_input: String }
AiSuggestion(String)
AiError(String)   // shown briefly in status bar, non-fatal
```

### Ghost text rendering

- `app_state.ai_suggestion: Option<String>` holds the current suggestion
- `render_input_bar` renders the suggestion text in `DarkGray` after the cursor when in INSERT mode
- `Tab` or `→` at end-of-input: appends suggestion to textarea, clears `ai_suggestion`
- Any other key: clears `ai_suggestion`

## Provider Trait

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn complete(
        &self,
        req: CompletionRequest,
        cancel: CancellationToken,
    ) -> Result<String>;
}

pub struct CompletionRequest {
    pub system: String,
    pub context: Vec<ContextMessage>,
    pub partial_input: String,
}
```

### Provider coverage

| Config value | Implementation | Covers |
|-------------|---------------|--------|
| `ollama`    | openai.rs     | Ollama, llama.cpp server |
| `openai`    | openai.rs     | OpenAI, Groq, Together, Mistral |
| `anthropic` | anthropic.rs  | Claude models |
| `gemini`    | gemini.rs     | Gemini models |

## Configuration

New `[ai]` section in `configs/default.toml`:

```toml
[ai]
enabled = false                          # opt-in, off by default
provider = "ollama"                      # ollama | openai | anthropic | gemini
base_url = "http://localhost:11434"      # for ollama/llama.cpp/openai-compat
api_key = ""                             # leave empty for local models
model = "qwen2.5:1.5b-instruct"

context_messages = 10                    # last N messages to include verbatim
summary_threshold = 50                   # regenerate summary every N messages
debounce_ms = 500                        # ms to wait after last keystroke
```

## Cost Estimate

| Mode                        | Cost per suggestion |
|-----------------------------|-------------------|
| Ollama / llama.cpp (local)  | $0                |
| OpenAI gpt-4o-mini          | ~$0.0001          |
| Anthropic claude-haiku-4-5  | ~$0.0001          |
| Gemini flash                | ~$0.00005         |

## Files to Create / Modify

| File | Change |
|------|--------|
| `src/ai/mod.rs` | New — AiWorker, request/event types |
| `src/ai/context.rs` | New — context builder, summary logic |
| `src/ai/providers/mod.rs` | New — AiProvider trait |
| `src/ai/providers/openai.rs` | New — OpenAI-compat client |
| `src/ai/providers/anthropic.rs` | New — Anthropic client |
| `src/ai/providers/gemini.rs` | New — Gemini client |
| `src/config/settings.rs` | Add `AiConfig` struct |
| `src/tui/app_state.rs` | Add `ai_suggestion: Option<String>` |
| `src/tui/event.rs` | Add `AiRequest`, `AiSuggestion`, `AiError` variants |
| `src/tui/widgets/input_bar.rs` | Render ghost text in DarkGray |
| `src/tui/keybindings.rs` | Add `Tab`/`Ctrl+Space` actions |
| `src/app.rs` | Spawn AiWorker, handle new events |
| `src/storage/preferences.rs` | Store/retrieve AI summary per chat |
| `Cargo.toml` | Add `reqwest`, `async-trait`, `tokio-util` deps |
