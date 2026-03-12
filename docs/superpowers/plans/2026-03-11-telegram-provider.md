# Telegram Provider Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `TelegramProvider` that plugs into the existing `MessagingProvider` trait, enabling Telegram DMs, groups, and channels in the unified TUI alongside WhatsApp.

**Architecture:** `grammers-client` (pure Rust MTProto user-account client) drives authentication (phone → OTP → optional 2FA), real-time updates, and message send/receive. Sessions persist to `~/.zero-drift-chat/telegram-session.db`. Auth input flows from the TUI to the provider task via an `mpsc::unbounded_channel::<String>()` stored in `App` — mirroring the existing `db_summary_tx/rx` pattern. A new centered modal overlay (`TelegramAuthMode`) handles all three auth stages identically to the existing settings/search overlays.

**Tech Stack:** Rust, `grammers-client` + `grammers-session` from Codeberg (`https://codeberg.org/Lonami/grammers`, v0.9.0), ratatui for the overlay, tokio for async tasks.

**Spec:** `docs/superpowers/specs/2026-03-11-telegram-provider-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `Cargo.toml` | Modify | Add `grammers-client`, `grammers-session` git deps from Codeberg |
| `configs/default.toml` | Modify | Add `[telegram]` section |
| `src/config/settings.rs` | Modify | Add `TelegramConfig` struct + wire into `AppConfig` |
| `src/core/provider.rs` | Modify | Add `AuthPhonePrompt`, `AuthOtpPrompt`, `AuthPasswordPrompt` `ProviderEvent` variants |
| `src/tui/event.rs` | Modify | Add `TelegramAuthInput(String)` `AppEvent` variant |
| `src/providers/mod.rs` | Modify | Add `pub mod telegram;` |
| `src/providers/telegram/mod.rs` | Create | `TelegramProvider` struct + `MessagingProvider` impl + update loop |
| `src/providers/telegram/convert.rs` | Create | `grammers` types → `UnifiedMessage` / `UnifiedChat`; unit tests |
| `src/tui/app_state.rs` | Modify | Add `TelegramAuthState`, `TelegramAuthStage`, `TelegramAuthMode` `InputMode` variant |
| `src/tui/keybindings.rs` | Modify | Add `TelegramAuthSubmit`, `TelegramAuthCancel` actions + `map_telegram_auth_mode` |
| `src/tui/widgets/telegram_auth_overlay.rs` | Create | Renders the phone/OTP/password modal |
| `src/tui/widgets/mod.rs` | Modify | Export `telegram_auth_overlay` |
| `src/tui/render.rs` | Modify | Call `telegram_auth_overlay::render_telegram_auth_overlay` when active |
| `src/app.rs` | Modify | Register `TelegramProvider`; store `telegram_auth_tx`; handle new events/actions |

---

## Chunk 1: Foundation — Dependencies, Config, Provider Events

### Task 1: Add `grammers` to `Cargo.toml`

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add grammers dependencies**

Open `Cargo.toml` and add these lines after the existing dependencies:

```toml
grammers-client = { git = "https://codeberg.org/Lonami/grammers", package = "grammers-client" }
grammers-session = { git = "https://codeberg.org/Lonami/grammers", package = "grammers-session" }
```

Also verify that `futures-util` is already a dependency (the update loop uses `StreamExt` from it). Check with:

```bash
grep "futures" Cargo.toml
```

If `futures-util` is not present, add it:

```toml
futures-util = "0.3"
```

- [ ] **Step 2: Verify it compiles (dependencies resolve)**

```bash
cargo fetch
```

Expected: no errors (crates download successfully). If Codeberg is unreachable, check network and try again.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add grammers-client and grammers-session from Codeberg"
```

---

### Task 2: Add `TelegramConfig` to `src/config/settings.rs`

**Files:**
- Modify: `src/config/settings.rs`
- Test: inline `#[cfg(test)]` at the bottom of `settings.rs`

- [ ] **Step 1: Write a failing test (TOML parsing)**

Add at the bottom of the `#[cfg(test)] mod tests` block in `src/config/settings.rs`:

```rust
#[test]
fn test_parse_telegram_config() {
    let toml = r#"
[telegram]
enabled = true
api_id = 123456
api_hash = "abc123def456"
"#;
    let result = toml::from_str::<AppConfig>(toml);
    assert!(result.is_ok(), "parse failed: {:?}", result.err());
    let cfg = result.unwrap();
    assert!(cfg.telegram.enabled);
    assert_eq!(cfg.telegram.api_id, 123456);
    assert_eq!(cfg.telegram.api_hash, "abc123def456");
}

#[test]
fn test_telegram_config_defaults() {
    let cfg = AppConfig::default();
    assert!(!cfg.telegram.enabled, "telegram disabled by default");
    assert_eq!(cfg.telegram.api_id, 0);
    assert!(cfg.telegram.api_hash.is_empty());
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test test_parse_telegram_config test_telegram_config_defaults 2>&1 | tail -20
```

Expected: compile error — `AppConfig` has no `telegram` field yet.

- [ ] **Step 3: Implement `TelegramConfig`**

In `src/config/settings.rs`, add after the `WhatsAppConfig` block (around line 63):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_id: i32,
    #[serde(default)]
    pub api_hash: String,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_id: 0,
            api_hash: String::new(),
        }
    }
}
```

Then add `telegram` field to `AppConfig`:

```rust
// In the AppConfig struct, after `whatsapp`:
#[serde(default)]
pub telegram: TelegramConfig,
```

And add `telegram: TelegramConfig::default()` to the `Default for AppConfig` impl. The `Default for AppConfig` impl is around line 156 in `settings.rs` and looks like this — add `telegram` to the existing block:

```rust
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            tui: TuiConfig::default(),
            mock_provider: MockProviderConfig::default(),
            whatsapp: WhatsAppConfig::default(),
            ai: AiConfig::default(),
            telegram: TelegramConfig::default(),  // ADD THIS LINE
        }
    }
}
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cargo test test_parse_telegram_config test_telegram_config_defaults 2>&1 | tail -10
```

Expected: both tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/config/settings.rs
git commit -m "feat: add TelegramConfig to AppConfig"
```

---

### Task 3: Add `[telegram]` section to `configs/default.toml`

**Files:**
- Modify: `configs/default.toml`

- [ ] **Step 1: Add telegram section**

Append to `configs/default.toml`:

```toml
[telegram]
enabled = false
# api_id and api_hash obtained from https://my.telegram.org (API development tools)
# api_id = 0
# api_hash = ""
```

- [ ] **Step 2: Verify config still loads (no regression)**

```bash
cargo test test_parse_ai_config 2>&1 | tail -5
```

Expected: PASS (existing test unaffected).

- [ ] **Step 3: Commit**

```bash
git add configs/default.toml
git commit -m "chore: add telegram section to default config"
```

---

### Task 4: Add new `ProviderEvent` variants to `src/core/provider.rs`

**Files:**
- Modify: `src/core/provider.rs`
- Modify: `src/tui/event.rs`

The auth flow requires the provider to request input from the TUI. Three new variants carry the auth stage and an optional error hint for retries.

- [ ] **Step 1: Add variants to `ProviderEvent`**

In `src/core/provider.rs`, find the `ProviderEvent` enum and add only these three new variants at the end (before the closing `}`). Do NOT replace the enum — just append these lines:

```rust
    // Telegram interactive auth — Option<String> carries retry error hint
    AuthPhonePrompt(Platform, Option<String>),
    AuthOtpPrompt(Platform, Option<String>),
    AuthPasswordPrompt(Platform, Option<String>),
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check 2>&1 | grep -E "^error"
```

Expected: no errors. (The new variants are never constructed yet — that's fine.)

- [ ] **Step 3: Add `TelegramAuthInput` to `AppEvent` in `src/tui/event.rs`**

```rust
// In AppEvent enum:
TelegramAuthInput(String),
```

- [ ] **Step 4: Verify again**

```bash
cargo check 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/core/provider.rs src/tui/event.rs
git commit -m "feat: add Telegram auth ProviderEvent and AppEvent variants"
```

---

## Chunk 2: Telegram Provider Implementation

### Task 5: Create `src/providers/telegram/convert.rs`

**Files:**
- Create: `src/providers/telegram/convert.rs`

This file owns all grammers→unified type conversions and the `PeerCache`. Keep it focused: no network calls, no async, pure data transformation.

- [ ] **Step 1: Write failing unit tests for conversion functions**

Create `src/providers/telegram/convert.rs` with only the test module first:

```rust
use crate::core::types::*;

// --- PeerCache (populated during get_chats, reused for send/get_messages) ---

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Maps our `tg-{peer_id}` chat IDs to grammers PackedChat handles.
/// Populated during `get_chats()`; reused in `send_message()` and `get_messages()`.
/// `PackedChat` is the correct grammers type to store — it implements `Into<PackedPeer>`
/// which all client methods accept via `C: Into<PackedPeer>`.
#[derive(Clone, Default)]
pub struct PeerCache {
    inner: Arc<Mutex<HashMap<String, grammers_client::types::PackedChat>>>,
}

impl PeerCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, chat_id: &str, peer: grammers_client::types::PackedChat) {
        self.inner.lock().unwrap().insert(chat_id.to_string(), peer);
    }

    pub fn get(&self, chat_id: &str) -> Option<grammers_client::types::PackedChat> {
        self.inner.lock().unwrap().get(chat_id).cloned()
    }
}

/// Encode a Telegram peer id (i64) to our chat_id string format.
pub fn peer_id_to_chat_id(peer_id: i64) -> String {
    format!("tg-{}", peer_id)
}

/// Decode our chat_id string back to a peer id (i64).
/// Returns None if the format is wrong.
pub fn chat_id_to_peer_id(chat_id: &str) -> Option<i64> {
    chat_id.strip_prefix("tg-")?.parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_id_round_trip_positive() {
        let id: i64 = 123456789;
        let chat_id = peer_id_to_chat_id(id);
        assert_eq!(chat_id, "tg-123456789");
        assert_eq!(chat_id_to_peer_id(&chat_id), Some(id));
    }

    #[test]
    fn test_chat_id_round_trip_negative() {
        // Telegram uses negative IDs for groups/channels
        let id: i64 = -1001234567890;
        let chat_id = peer_id_to_chat_id(id);
        assert_eq!(chat_id, "tg--1001234567890");
        assert_eq!(chat_id_to_peer_id(&chat_id), Some(id));
    }

    #[test]
    fn test_chat_id_invalid() {
        assert_eq!(chat_id_to_peer_id("wa-12345"), None);
        assert_eq!(chat_id_to_peer_id("tg-notanumber"), None);
        assert_eq!(chat_id_to_peer_id(""), None);
    }
}
```

- [ ] **Step 2: Run tests to confirm they fail (module not registered yet)**

```bash
cargo test test_chat_id 2>&1 | tail -10
```

Expected: compile error — module not found. Good — proceed to wire it up.

- [ ] **Step 3: Register the module**

Create `src/providers/telegram/mod.rs` (minimal stub for now):

```rust
pub mod convert;
```

And add to `src/providers/mod.rs`:

```rust
pub mod telegram;
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cargo test test_chat_id 2>&1 | tail -10
```

Expected: 3 tests PASS.

- [ ] **Step 5: Add `grammers_message_to_unified` and `grammers_dialog_to_chat` stubs + tests**

Append to `src/providers/telegram/convert.rs` (after the `PeerCache` block, before `#[cfg(test)]`):

```rust
/// Convert a grammers `Message` to `UnifiedMessage`.
/// Returns `None` if the message has no usable text content (e.g., service messages we skip).
pub fn grammers_message_to_unified(
    msg: &grammers_client::types::Message,
    chat_id: &str,
) -> Option<UnifiedMessage> {
    // Text content — media becomes "[Media]" placeholder (v1)
    let text = if msg.text().is_empty() {
        "[Media]".to_string()
    } else {
        msg.text().to_string()
    };

    let sender = msg
        .sender()
        .map(|s| s.name().to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    Some(UnifiedMessage {
        id: msg.id().to_string(),
        chat_id: chat_id.to_string(),
        platform: Platform::Telegram,
        sender,
        content: MessageContent::Text(text),
        timestamp: {
            use chrono::TimeZone;
            chrono::Utc.timestamp_opt(msg.date().timestamp(), 0)
                .single()
                .unwrap_or_else(chrono::Utc::now)
        },
        status: MessageStatus::Sent,
        is_outgoing: msg.outgoing(),
    })
}
```

Add a unit test for `grammers_message_to_unified`:

```rust
// NOTE: grammers types cannot be constructed in unit tests — their
// constructors are private. The chat-id encode/decode round-trip tests
// above are the meaningful unit coverage. Integration behaviour is
// verified manually via the live provider.
//
// If grammers ever exposes test helpers, add message conversion tests here.
```

(This comment is intentional — document why there is no test. The test for dialog conversion will follow the same pattern.)

- [ ] **Step 6: Commit**

```bash
git add src/providers/mod.rs src/providers/telegram/mod.rs src/providers/telegram/convert.rs
git commit -m "feat: add TelegramProvider module scaffold and convert.rs with chat-id helpers"
```

---

### Task 6: Implement `TelegramProvider` in `src/providers/telegram/mod.rs`

**Files:**
- Modify: `src/providers/telegram/mod.rs`

- [ ] **Step 1: Verify the grammers API surface we need**

Run a quick check to ensure the dependency resolved correctly:

```bash
cargo doc --no-deps -p grammers-client 2>&1 | tail -5
```

Expected: documentation generated, no build errors.

- [ ] **Step 2: Write the full `TelegramProvider` struct and `new()`**

Replace the stub `src/providers/telegram/mod.rs` with:

```rust
pub mod convert;

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use grammers_client::{Client, Config, SignInError};
use grammers_client::session::storages::SqliteSession;
use grammers_client::types::message::InputMessage;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::core::provider::{MessagingProvider, ProviderEvent};
use crate::core::types::*;
use crate::core::Result;

use convert::{
    chat_id_to_peer_id, grammers_message_to_unified, peer_id_to_chat_id, PeerCache,
};

const MAX_AUTH_RETRIES: u8 = 3;
const MAX_DIALOGS: usize = 200;
const MAX_MESSAGES: usize = 50;

pub struct TelegramProvider {
    client: Option<Arc<Client>>,
    update_handle: Option<JoinHandle<()>>,
    tx: Option<mpsc::UnboundedSender<ProviderEvent>>,
    auth_status: AuthStatus,
    session_path: String,
    api_id: i32,
    api_hash: String,
    /// Sender end: cloned into `App` as `telegram_auth_tx`.
    pub auth_tx: mpsc::UnboundedSender<String>,
    /// Receiver end: auth task awaits on this for user's typed input.
    auth_rx: Arc<Mutex<mpsc::UnboundedReceiver<String>>>,
    peer_cache: PeerCache,
}

impl TelegramProvider {
    pub fn new(session_path: String, api_id: i32, api_hash: String) -> Self {
        let (auth_tx, auth_rx) = mpsc::unbounded_channel::<String>();
        Self {
            client: None,
            update_handle: None,
            tx: None,
            auth_status: AuthStatus::NotAuthenticated,
            session_path,
            api_id,
            api_hash,
            auth_tx,
            auth_rx: Arc::new(Mutex::new(auth_rx)),
            peer_cache: PeerCache::new(),
        }
    }
}
```

- [ ] **Step 3: Implement `MessagingProvider::start()`**

Append to `mod.rs`:

```rust
#[async_trait]
impl MessagingProvider for TelegramProvider {
    async fn start(&mut self, tx: mpsc::UnboundedSender<ProviderEvent>) -> Result<()> {
        self.auth_status = AuthStatus::Authenticating;
        let _ = tx.send(ProviderEvent::AuthStatusChanged(
            Platform::Telegram,
            AuthStatus::Authenticating,
        ));

        // Load or create session — SqliteSession::open() creates the file if it doesn't exist
        let session = SqliteSession::open(&self.session_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to open Telegram session: {}", e))?;

        let client = Client::new(Config {
            session,
            api_id: self.api_id,
            api_hash: self.api_hash.clone(),
            params: Default::default(),
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to Telegram: {}", e))?;

        let client = Arc::new(client);

        // --- Authentication ---
        if !client.is_authorized().await.map_err(|e| anyhow::anyhow!("{}", e))? {
            self.authenticate(Arc::clone(&client), &tx).await?;
        }

        // Session auto-saves on Client drop — no explicit save needed

        let _ = tx.send(ProviderEvent::AuthStatusChanged(
            Platform::Telegram,
            AuthStatus::Authenticated,
        ));
        self.auth_status = AuthStatus::Authenticated;

        // Spawn update loop
        let client_clone = Arc::clone(&client);
        let tx_clone = tx.clone();
        let handle = tokio::spawn(async move {
            run_update_loop(client_clone, tx_clone).await;
        });

        self.client = Some(client);
        self.update_handle = Some(handle);
        self.tx = Some(tx);

        tracing::info!("Telegram provider started");
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(handle) = self.update_handle.take() {
            handle.abort();
        }
        self.client = None;
        self.auth_status = AuthStatus::NotAuthenticated;
        tracing::info!("Telegram provider stopped");
        Ok(())
    }

    async fn send_message(&self, chat_id: &str, content: MessageContent) -> Result<UnifiedMessage> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Telegram client not connected"))?;

        let peer = self
            .peer_cache
            .get(chat_id)
            .ok_or_else(|| anyhow::anyhow!("No cached peer for chat_id: {}", chat_id))?;

        let text = match &content {
            MessageContent::Text(t) => t.clone(),
            other => other.as_text().to_string(),
        };

        client
            .send_message(&peer, InputMessage::text(&text))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send Telegram message: {}", e))?;

        let unified = UnifiedMessage {
            id: uuid::Uuid::new_v4().to_string(), // provisional; updated on echo
            chat_id: chat_id.to_string(),
            platform: Platform::Telegram,
            sender: "You".to_string(),
            content,
            timestamp: Utc::now(),
            status: MessageStatus::Sent,
            is_outgoing: true,
        };

        if let Some(tx) = &self.tx {
            let _ = tx.send(ProviderEvent::NewMessage(unified.clone()));
        }

        Ok(unified)
    }

    async fn get_chats(&self) -> Result<Vec<UnifiedChat>> {
        let client = match &self.client {
            Some(c) => c,
            None => return Ok(Vec::new()),
        };

        let mut dialogs = client.iter_dialogs();
        let mut chats = Vec::new();

        let mut count = 0;
        while let Some(dialog) = dialogs
            .next()
            .await
            .map_err(|e| anyhow::anyhow!("Error iterating Telegram dialogs: {}", e))?
        {
            if count >= MAX_DIALOGS {
                break;
            }
            count += 1;

            let chat = dialog.chat();
            let peer_id = chat.id();
            let chat_id = peer_id_to_chat_id(peer_id);

            // Cache the PackedChat for send_message/get_messages
            // `chat.pack()` returns a `PackedChat` which implements `Into<PackedPeer>`
            self.peer_cache.insert(&chat_id, chat.pack());

            let is_channel = matches!(chat, grammers_client::types::Chat::Channel(_));
            let is_broadcast = is_channel && {
                if let grammers_client::types::Chat::Channel(ch) = chat {
                    ch.broadcast()
                } else {
                    false
                }
            };
            let is_group = matches!(
                chat,
                grammers_client::types::Chat::Group(_) | grammers_client::types::Chat::Channel(_)
            ) && !is_broadcast;

            let last_message = dialog
                .latest_message()
                .and_then(|m| grammers_message_to_unified(m, &chat_id))
                .map(|m| m.content.as_text().to_string());

            chats.push(UnifiedChat {
                id: chat_id,
                platform: Platform::Telegram,
                name: chat.name().to_string(),
                display_name: None,
                last_message,
                unread_count: dialog.unread_message_count() as u32,
                is_group,
                is_pinned: dialog.pinned(),
                is_newsletter: is_broadcast,
                is_muted: false,
            });
        }

        Ok(chats)
    }

    async fn get_messages(&self, chat_id: &str) -> Result<Vec<UnifiedMessage>> {
        let client = match &self.client {
            Some(c) => c,
            None => return Ok(Vec::new()),
        };

        let peer = match self.peer_cache.get(chat_id) {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };

        let mut iter = client.iter_messages(&peer).limit(MAX_MESSAGES as u32);
        let mut messages = Vec::new();

        while let Some(msg) = iter
            .next()
            .await
            .map_err(|e| anyhow::anyhow!("Error iterating Telegram messages: {}", e))?
        {
            if let Some(unified) = grammers_message_to_unified(&msg, chat_id) {
                messages.push(unified);
            }
        }

        // grammers returns newest-first; reverse to chronological order
        messages.reverse();
        Ok(messages)
    }

    async fn mark_as_read(&self, chat_id: &str, _msg_ids: Vec<String>) -> Result<()> {
        let client = match &self.client {
            Some(c) => c,
            None => return Ok(()),
        };

        let peer = match self.peer_cache.get(chat_id) {
            Some(p) => p,
            None => return Ok(()),
        };

        client
            .mark_as_read(&peer)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to mark Telegram chat as read: {}", e))?;

        Ok(())
    }

    fn name(&self) -> &str {
        "Telegram"
    }

    fn platform(&self) -> Platform {
        Platform::Telegram
    }

    fn auth_status(&self) -> AuthStatus {
        self.auth_status
    }
}
```

- [ ] **Step 4: Implement the `authenticate()` helper**

Append to `mod.rs`:

```rust
impl TelegramProvider {
    /// Drive the interactive phone→OTP→2FA auth flow.
    /// Sends `AuthXxxPrompt` events to the TUI and awaits user input via `auth_rx`.
    async fn authenticate(
        &self,
        client: Arc<Client>,
        tx: &mpsc::UnboundedSender<ProviderEvent>,
    ) -> Result<()> {
        // --- Step 1: Phone number ---
        let _ = tx.send(ProviderEvent::AuthPhonePrompt(Platform::Telegram, None));
        let phone = self.await_auth_input().await?;

        let token = client
            .request_login_code(&phone)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to request Telegram login code: {}", e))?;

        // Note: api_hash is already provided via Config at Client::new() — do NOT pass it again here.

        // --- Step 2: OTP ---
        let mut otp_retries = 0u8;
        let signed_in = loop {
            let _ = tx.send(ProviderEvent::AuthOtpPrompt(Platform::Telegram, None));
            let otp = self.await_auth_input().await?;

            match client.sign_in(&token, &otp).await {
                Ok(user) => break Ok(user),
                Err(SignInError::PasswordRequired(pw_token)) => {
                    // 2FA required — pw_token comes directly from the error variant;
                    // no second network call needed
                    break Err(SignInError::PasswordRequired(pw_token))
                }
                Err(SignInError::InvalidCode) => {
                    otp_retries += 1;
                    if otp_retries >= MAX_AUTH_RETRIES {
                        let _ = tx.send(ProviderEvent::AuthStatusChanged(
                            Platform::Telegram,
                            AuthStatus::Failed,
                        ));
                        return Err(anyhow::anyhow!("Too many wrong OTP attempts"));
                    }
                    let _ = tx.send(ProviderEvent::AuthOtpPrompt(
                        Platform::Telegram,
                        Some("Wrong code, try again".to_string()),
                    ));
                    continue;
                }
                Err(e) => {
                    let _ = tx.send(ProviderEvent::AuthStatusChanged(
                        Platform::Telegram,
                        AuthStatus::Failed,
                    ));
                    return Err(anyhow::anyhow!("Telegram sign-in error: {}", e));
                }
            }
        };

        match signed_in {
            Ok(_) => {
                tracing::info!("Telegram signed in successfully");
                Ok(())
            }
            Err(SignInError::PasswordRequired(pw_token)) => {
                // --- Step 3: 2FA ---
                // pw_token is consumed by check_password on each call, so we loop
                // differently: only one attempt per token; if wrong, the error
                // returns a new PasswordToken we must use on the next attempt.
                let mut current_token = pw_token;
                let mut pw_retries = 0u8;
                loop {
                    let _ = tx.send(ProviderEvent::AuthPasswordPrompt(Platform::Telegram, None));
                    let password = self.await_auth_input().await?;

                    match client.check_password(current_token, &password).await {
                        Ok(_) => {
                            tracing::info!("Telegram 2FA passed");
                            return Ok(());
                        }
                        Err(e) => {
                            pw_retries += 1;
                            if pw_retries >= MAX_AUTH_RETRIES {
                                let _ = tx.send(ProviderEvent::AuthStatusChanged(
                                    Platform::Telegram,
                                    AuthStatus::Failed,
                                ));
                                return Err(anyhow::anyhow!("Too many wrong 2FA attempts"));
                            }
                            // check_password on wrong password returns a new PasswordToken
                            // embedded in the error; extract it for the next attempt.
                            if let grammers_client::SignInError::PasswordRequired(new_token) = e {
                                current_token = new_token;
                            } else {
                                let _ = tx.send(ProviderEvent::AuthStatusChanged(
                                    Platform::Telegram,
                                    AuthStatus::Failed,
                                ));
                                return Err(anyhow::anyhow!("Telegram 2FA error: {}", e));
                            }
                            let _ = tx.send(ProviderEvent::AuthPasswordPrompt(
                                Platform::Telegram,
                                Some("Wrong password, try again".to_string()),
                            ));
                        }
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(ProviderEvent::AuthStatusChanged(
                    Platform::Telegram,
                    AuthStatus::Failed,
                ));
                Err(anyhow::anyhow!("Telegram auth error: {}", e))
            }
        }
    }

    /// Wait for a single auth input string from the TUI via the auth channel.
    async fn await_auth_input(&self) -> Result<String> {
        self.auth_rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("Auth input channel closed"))
    }
}
```

- [ ] **Step 5: Implement the update loop function**

Append to `mod.rs`:

```rust
/// Runs in a spawned task: forwards grammers updates to the provider event channel.
async fn run_update_loop(
    client: Arc<Client>,
    tx: mpsc::UnboundedSender<ProviderEvent>,
) {
    use futures_util::StreamExt;
    use grammers_client::Update;

    let mut update_stream = client.stream_updates();
    let mut backoff_secs = 1u64;
    let max_backoff = 4u64;
    let mut consecutive_errors = 0u8;

    loop {
        match update_stream.next().await {
            Some(Ok(update)) => {
                backoff_secs = 1;
                consecutive_errors = 0;

                match update {
                    Update::NewMessage(msg) if !msg.outgoing() => {
                        let chat_id = peer_id_to_chat_id(msg.chat().id());
                        if let Some(unified) = grammers_message_to_unified(&msg, &chat_id) {
                            let _ = tx.send(ProviderEvent::NewMessage(unified));
                        }
                    }
                    Update::MessageEdited(msg) => {
                        // Re-emit as new message (v1 simplification)
                        let chat_id = peer_id_to_chat_id(msg.chat().id());
                        if let Some(unified) = grammers_message_to_unified(&msg, &chat_id) {
                            let _ = tx.send(ProviderEvent::NewMessage(unified));
                        }
                    }
                    _ => {
                        // `Update` is #[non_exhaustive] — must have catch-all arm.
                        // Read receipts and other update types are not exposed as
                        // top-level enum variants in grammers 0.9.0.
                        tracing::trace!("Unhandled Telegram update");
                    }
                }
                // Session auto-saves on Client drop — no explicit save needed here.
            }
            Some(Err(e)) => {
                tracing::error!("Telegram update error: {}", e);
                consecutive_errors += 1;
                if consecutive_errors >= 3 {
                    let _ = tx.send(ProviderEvent::AuthStatusChanged(
                        Platform::Telegram,
                        AuthStatus::Failed,
                    ));
                    tracing::error!("Telegram: 3 consecutive update errors, stopping loop");
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(max_backoff);
            }
            None => {
                tracing::info!("Telegram update stream ended");
                break;
            }
        }
    }
}
```

- [ ] **Step 6: Verify it compiles**

```bash
cargo check 2>&1 | grep -E "^error"
```

Expected: no errors. If any grammers method signatures differ from what the plan specifies, consult the generated docs:

```bash
cargo doc --no-deps -p grammers-client 2>&1 | tail -5
# then open target/doc/grammers_client/index.html
```

- [ ] **Step 7: Commit**

```bash
git add src/providers/telegram/mod.rs
git commit -m "feat: implement TelegramProvider with auth, send, get_chats, get_messages"
```

---

## Chunk 3: TUI Integration — Auth Overlay, Keybindings, App Wiring

### Task 7: Add `TelegramAuthMode` to `InputMode` and `TelegramAuthState` to `app_state.rs`

**Files:**
- Modify: `src/tui/app_state.rs`

- [ ] **Step 1: Add new types**

In `src/tui/app_state.rs`:

Add to `InputMode` enum:
```rust
TelegramAuth,
```

Add new types after the `SearchState` block (around line 210):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegramAuthStage {
    Phone,
    Otp,
    Password,
}

impl TelegramAuthStage {
    pub fn prompt(&self) -> &'static str {
        match self {
            TelegramAuthStage::Phone => "Enter your Telegram phone number (with country code, e.g. +1234567890):",
            TelegramAuthStage::Otp => "Enter the code Telegram sent you:",
            TelegramAuthStage::Password => "Enter your 2FA password:",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            TelegramAuthStage::Phone => " Telegram Login — Phone ",
            TelegramAuthStage::Otp => " Telegram Login — Code ",
            TelegramAuthStage::Password => " Telegram Login — 2FA Password ",
        }
    }

    pub fn is_password(&self) -> bool {
        matches!(self, TelegramAuthStage::Password)
    }
}

pub struct TelegramAuthState {
    pub stage: TelegramAuthStage,
    pub input: String,
    /// Optional error hint from retry (e.g. "Wrong code, try again")
    pub error_hint: Option<String>,
}

impl TelegramAuthState {
    pub fn new(stage: TelegramAuthStage, error_hint: Option<String>) -> Self {
        Self {
            stage,
            input: String::new(),
            error_hint,
        }
    }
}
```

Add `telegram_auth_state: Option<TelegramAuthState>` field to `AppState` struct (after `search_state`):

```rust
pub telegram_auth_state: Option<TelegramAuthState>,
```

Initialize to `None` in `AppState::new()`.

Add helper methods to `AppState`:

```rust
pub fn open_telegram_auth(&mut self, stage: TelegramAuthStage, error_hint: Option<String>) {
    self.telegram_auth_state = Some(TelegramAuthState::new(stage, error_hint));
    self.input_mode = InputMode::TelegramAuth;
}

pub fn close_telegram_auth(&mut self) {
    self.telegram_auth_state = None;
    self.input_mode = InputMode::Normal;
}

pub fn take_telegram_auth_input(&mut self) -> String {
    if let Some(ref mut auth) = self.telegram_auth_state {
        std::mem::take(&mut auth.input)
    } else {
        String::new()
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/tui/app_state.rs
git commit -m "feat: add TelegramAuthState and TelegramAuth InputMode"
```

---

### Task 8: Add keybinding actions for `TelegramAuth` mode

**Files:**
- Modify: `src/tui/keybindings.rs`

- [ ] **Step 1: Add actions**

In `src/tui/keybindings.rs`:

Add to `Action` enum:
```rust
TelegramAuthChar(char),
TelegramAuthBackspace,
TelegramAuthSubmit,
TelegramAuthCancel,
```

Add to `map_key()` match:
```rust
InputMode::TelegramAuth => map_telegram_auth_mode(key),
```

Add mapping function:
```rust
fn map_telegram_auth_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::TelegramAuthCancel,
        KeyCode::Enter => Action::TelegramAuthSubmit,
        KeyCode::Backspace => Action::TelegramAuthBackspace,
        KeyCode::Char(c) => Action::TelegramAuthChar(c),
        _ => Action::None,
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/tui/keybindings.rs
git commit -m "feat: add TelegramAuth keybinding actions"
```

---

### Task 9: Create `src/tui/widgets/telegram_auth_overlay.rs`

**Files:**
- Create: `src/tui/widgets/telegram_auth_overlay.rs`
- Modify: `src/tui/widgets/mod.rs`

- [ ] **Step 1: Create the widget**

```rust
use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::app_state::{TelegramAuthStage, TelegramAuthState};

pub fn render_telegram_auth_overlay(f: &mut Frame, state: &TelegramAuthState) {
    let area = f.area();

    let popup_width = 60u16.min(area.width);
    let popup_height = 10u16.min(area.height);
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup = ratatui::layout::Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup);

    let border_color = match state.stage {
        TelegramAuthStage::Phone => Color::Cyan,
        TelegramAuthStage::Otp => Color::Yellow,
        TelegramAuthStage::Password => Color::Magenta,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(state.stage.title())
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(""));

    // Prompt
    lines.push(Line::from(Span::styled(
        format!("  {}", state.stage.prompt()),
        Style::default().fg(Color::White),
    )));

    lines.push(Line::from(""));

    // Error hint (shown on retry)
    if let Some(ref hint) = state.error_hint {
        lines.push(Line::from(Span::styled(
            format!("  ! {}", hint),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    } else {
        lines.push(Line::from(""));
    }

    lines.push(Line::from(""));

    // Input field (mask password input)
    let display_input = if state.stage.is_password() {
        "*".repeat(state.input.len())
    } else {
        state.input.clone()
    };
    lines.push(Line::from(vec![
        Span::styled("  > ", Style::default().fg(border_color)),
        Span::styled(display_input, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("_", Style::default().fg(Color::DarkGray)), // cursor
    ]));

    lines.push(Line::from(""));

    // Footer hints
    lines.push(Line::from(Span::styled(
        "  Enter: Submit   Esc: Cancel",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}
```

- [ ] **Step 2: Export from `mod.rs`**

In `src/tui/widgets/mod.rs`, add:
```rust
pub mod telegram_auth_overlay;
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/tui/widgets/telegram_auth_overlay.rs src/tui/widgets/mod.rs
git commit -m "feat: add Telegram auth overlay widget"
```

---

### Task 10: Wire the overlay into `src/tui/render.rs`

**Files:**
- Modify: `src/tui/render.rs`

- [ ] **Step 1: Add render call**

In `src/tui/render.rs`:

1. Find the `use super::widgets::{self, ...}` import near the top of the file and add `telegram_auth_overlay` to the list (follow the same pattern as `qr_overlay`).

2. Find the block that renders other overlays (after the search overlay, around line 143–147). Add this block immediately after the search overlay render:

```rust
// Render Telegram auth overlay on top if active
if let Some(ref auth_state) = state.telegram_auth_state {
    telegram_auth_overlay::render_telegram_auth_overlay(f, auth_state);
}
```

The `telegram_auth_overlay` is accessible via the `use super::widgets::{..., telegram_auth_overlay}` import you just added.

- [ ] **Step 2: Verify it compiles**

```bash
cargo check 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/tui/render.rs
git commit -m "feat: render Telegram auth overlay in TUI"
```

---

### Task 11: Wire `TelegramProvider` into `src/app.rs`

**Files:**
- Modify: `src/app.rs`

This is the largest wiring task. Work through it methodically.

- [ ] **Step 1: Add `telegram_auth_tx` field to `App` struct**

In `src/app.rs`, add to the `App` struct:

```rust
telegram_auth_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
```

Initialize to `None` in `App::new()`.

- [ ] **Step 2: Register `TelegramProvider` in `App::run()`**

In the provider registration block (after `whatsapp` registration, around line 119):

```rust
if self.config.telegram.enabled {
    let api_id = self.config.telegram.api_id;
    let api_hash = self.config.telegram.api_hash.clone();

    if api_id == 0 || api_hash.is_empty() {
        tracing::error!(
            "Telegram enabled but api_id or api_hash not configured — skipping"
        );
    } else {
        let session_path = format!(
            "{}/telegram-session.db",
            self.config.general.data_dir
        );
        let tg = crate::providers::telegram::TelegramProvider::new(
            session_path,
            api_id,
            api_hash,
        );
        // Stash the auth_tx so we can forward TUI input to the provider's auth task
        self.telegram_auth_tx = Some(tg.auth_tx.clone());
        self.router.register_provider(Box::new(tg));
    }
}
```

Also add `Platform::Telegram` to the chat filter in the same function (find the `match c.platform` block around line 131):

```rust
Platform::Telegram => self.config.telegram.enabled,
```

- [ ] **Step 3: Handle new `ProviderEvent` variants in `handle_tick()`**

In `handle_tick()`, find the existing `AuthStatusChanged` arm and **replace it** with the version below (which handles both WhatsApp and Telegram). Also add the three new `AuthXxxPrompt` arms — add them after the `AuthQrCode` arm:

```rust
ProviderEvent::AuthPhonePrompt(platform, error_hint) => {
    if platform == Platform::Telegram {
        self.state.open_telegram_auth(
            crate::tui::app_state::TelegramAuthStage::Phone,
            error_hint,
        );
    }
}
ProviderEvent::AuthOtpPrompt(platform, error_hint) => {
    if platform == Platform::Telegram {
        self.state.open_telegram_auth(
            crate::tui::app_state::TelegramAuthStage::Otp,
            error_hint,
        );
    }
}
ProviderEvent::AuthPasswordPrompt(platform, error_hint) => {
    if platform == Platform::Telegram {
        self.state.open_telegram_auth(
            crate::tui::app_state::TelegramAuthStage::Password,
            error_hint,
        );
    }
}
```

Replace the **existing** `AuthStatusChanged` arm (do NOT add a duplicate arm) with:

```rust
ProviderEvent::AuthStatusChanged(platform, status) => {
    tracing::info!("Auth status changed for {:?}: {:?}", platform, status);
    if platform == Platform::WhatsApp {
        match status {
            AuthStatus::Authenticated => {
                self.state.whatsapp_connected = true;
                self.state.qr_code = None;
            }
            AuthStatus::NotAuthenticated | AuthStatus::Failed => {
                self.state.whatsapp_connected = false;
            }
            _ => {}
        }
    }
    if platform == Platform::Telegram {
        match status {
            AuthStatus::Authenticated => {
                self.state.close_telegram_auth();
            }
            AuthStatus::Failed => {
                self.state.close_telegram_auth();
                // Optionally surface error in status bar (future enhancement)
                tracing::error!("Telegram authentication failed");
            }
            _ => {}
        }
    }
}
```

- [ ] **Step 4: Handle new `Action` variants in `handle_action()`**

Add to the `match action` block:

```rust
Action::TelegramAuthChar(c) => {
    if let Some(ref mut auth) = self.state.telegram_auth_state {
        auth.input.push(c);
    }
}
Action::TelegramAuthBackspace => {
    if let Some(ref mut auth) = self.state.telegram_auth_state {
        auth.input.pop();
    }
}
Action::TelegramAuthSubmit => {
    let value = self.state.take_telegram_auth_input();
    if !value.is_empty() {
        if let Some(ref tx) = self.telegram_auth_tx {
            let _ = tx.send(value);
        }
        // Close overlay; it will re-open if auth needs another step
        self.state.close_telegram_auth();
    }
}
Action::TelegramAuthCancel => {
    self.state.close_telegram_auth();
    tracing::info!("Telegram auth cancelled by user");
}
```

- [ ] **Step 5: Add the `TelegramAuthInput` `AppEvent` branch**

In the main event loop, add a branch for `AppEvent::TelegramAuthInput` — this forwards auth input received via the event system directly to the provider's auth channel:

```rust
Some(AppEvent::TelegramAuthInput(value)) => {
    if let Some(ref tx) = self.telegram_auth_tx {
        let _ = tx.send(value);
    }
}
```

- [ ] **Step 6: Verify it compiles**

```bash
cargo check 2>&1 | grep -E "^error"
```

Expected: no errors. Fix any missing imports:
- Add `use crate::providers::telegram::TelegramProvider;` at the top of `app.rs` if needed (may be accessed via full path instead — that's fine too).
- Add `use crate::core::types::Platform;` if not already imported.

- [ ] **Step 7: Run all tests**

```bash
cargo test 2>&1 | tail -20
```

Expected: all existing tests pass, plus the new Telegram config and chat-id tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/app.rs
git commit -m "feat: wire TelegramProvider into App — auth events, auth_tx channel, action handlers"
```

---

### Task 12: Manual integration test

This cannot be automated (requires real Telegram credentials). Document the manual test procedure here.

**Prerequisites:**
1. Obtain `api_id` and `api_hash` from https://my.telegram.org → API development tools
2. Set in your local config:
   ```toml
   [telegram]
   enabled = true
   api_id = <your_api_id>
   api_hash = "<your_api_hash>"
   ```
3. Disable mock provider to reduce noise:
   ```toml
   [mock_provider]
   enabled = false
   ```

**Test scenarios:**

- [ ] **Scenario 1: Fresh login (no session file)**
  1. Delete `~/.zero-drift-chat/telegram-session.db` if it exists
  2. Run: `cargo run`
  3. Expected: phone number overlay appears immediately
  4. Enter phone number → OTP overlay appears
  5. Enter OTP → (if 2FA enabled) password overlay appears
  6. After auth: `telegram-session.db` created, chats load, status bar shows Telegram active
  7. Verify: can navigate chats, messages show in message view

- [ ] **Scenario 2: Session resume (session file exists)**
  1. Run `cargo run` again (after Scenario 1)
  2. Expected: no auth overlay; Telegram chats load directly

- [ ] **Scenario 3: Send a message**
  1. Select a Telegram chat
  2. Press `i`, type a message, press Enter
  3. Expected: message appears in chat view, sent to Telegram

- [ ] **Scenario 4: Wrong OTP**
  1. Delete session file, restart
  2. Enter correct phone, then enter `000000` as OTP
  3. Expected: overlay re-opens with red "Wrong code, try again" hint
  4. On 3rd failure: overlay closes, Telegram marked as failed

- [ ] **Scenario 5: Cancel auth**
  1. Delete session file, restart
  2. When phone overlay appears, press `Esc`
  3. Expected: overlay closes, Telegram inactive, WhatsApp/mock still works

- [ ] **Step: Commit final manual test documentation**

```bash
git add docs/superpowers/plans/2026-03-11-telegram-provider.md
git commit -m "docs: add Telegram provider implementation plan"
```

---

## Summary

| Task | Files Changed | Tests |
|------|--------------|-------|
| 1. Add grammers deps | `Cargo.toml` | cargo fetch |
| 2. TelegramConfig | `settings.rs` | 2 unit tests |
| 3. default.toml | `configs/default.toml` | existing test unaffected |
| 4. ProviderEvent/AppEvent | `provider.rs`, `event.rs` | cargo check |
| 5. convert.rs scaffold | `telegram/convert.rs`, `mod.rs` | 3 unit tests |
| 6. TelegramProvider | `telegram/mod.rs` | cargo check |
| 7. AppState auth types | `app_state.rs` | cargo check |
| 8. Keybindings | `keybindings.rs` | cargo check |
| 9. Auth overlay widget | `telegram_auth_overlay.rs`, `widgets/mod.rs` | cargo check |
| 10. Render wiring | `render.rs` | cargo check |
| 11. App wiring | `app.rs` | `cargo test` all pass |
| 12. Manual test | — | manual |
