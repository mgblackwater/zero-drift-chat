# Telegram Provider Design Spec

**Date:** 2026-03-11  
**Feature:** Dual message app support — Telegram  
**Approach:** Approach A — Direct `grammers` integration with interactive TUI auth  

---

## Problem Statement

`zero-drift-chat` currently supports WhatsApp only. Users want a unified TUI that aggregates multiple messaging platforms. Telegram is the first additional platform. The goal is to add a `TelegramProvider` that plugs into the existing `MessagingProvider` trait without changing the core architecture.

---

## Approach

Use [`grammers-client`](https://github.com/Lonami/grammers) — a pure Rust MTProto client — to connect to Telegram as a user account (not a bot). This supports DMs, groups, and channels. Interactive authentication (phone → OTP → optional 2FA) is handled inside the TUI via a new auth overlay, mirroring the WhatsApp QR code flow. Sessions are persisted to `~/.zero-drift-chat/telegram-session.db`.

Rejected alternatives:
- **teloxide**: Bot API only — cannot read personal chats.
- **tdlib bindings**: Heavy C++ build dependency, overkill for v1.
- **Pre-loaded session file**: Poor UX, breaks the unified-TUI philosophy.

---

## Architecture

### New files

```
src/providers/telegram/
  mod.rs      — TelegramProvider struct + MessagingProvider impl
  convert.rs  — grammers types → UnifiedMessage / UnifiedChat
```

`TelegramProvider` struct mirrors `WhatsAppProvider`:
- `client: Option<Arc<grammers_client::Client>>`
- `update_handle: Option<JoinHandle<()>>`
- `tx: Option<mpsc::UnboundedSender<ProviderEvent>>`
- `auth_status: AuthStatus`
- `session_path: String`
- `auth_tx: mpsc::UnboundedSender<String>` — channel for TUI to send auth inputs

### Modified files

- `src/providers/mod.rs` — add `pub mod telegram;`
- `src/core/provider.rs` — add new `ProviderEvent` variants (see Auth section)
- `src/tui/app_state.rs` — add `TelegramAuthState` for the auth overlay
- `src/app.rs` — register `TelegramProvider` if enabled; handle new auth events
- `src/config/settings.rs` — add `TelegramConfig`
- `Cargo.toml` — add `grammers-client`, `grammers-session` dependencies

---

## Configuration

New section in `configs/default.toml`:

```toml
[telegram]
enabled = false
api_id = 0
api_hash = ""
```

`api_id` and `api_hash` are obtained from https://my.telegram.org (under "API development tools"). If `enabled = true` but `api_id = 0` or `api_hash` is empty, the provider logs an error and is skipped — no crash.

New `TelegramConfig` struct in `settings.rs`:

```rust
pub struct TelegramConfig {
    pub enabled: bool,
    pub api_id: i32,
    pub api_hash: String,
}
```

Default: `enabled = false`.

---

## Authentication Flow

`grammers` requires up to three interactive steps when no session exists:

1. Phone number
2. OTP code (sent via Telegram)
3. 2FA password (only if cloud password is set)

### New `ProviderEvent` variants

```rust
AuthPhonePrompt(Platform),      // provider needs phone number
AuthOtpPrompt(Platform),        // provider needs OTP
AuthPasswordPrompt(Platform),   // provider needs 2FA password
```

### New `AppEvent` variant

```rust
TelegramAuthInput(String),      // TUI submits user's typed value to provider
```

### Sequence

```
Provider::start()
  └─ no session → send AuthPhonePrompt(Telegram)
     TUI: show modal "Enter Telegram phone number (+country code)"
     User submits → TelegramAuthInput(phone) → provider via auth_tx channel
  └─ provider sends OTP to user's Telegram → send AuthOtpPrompt(Telegram)
     TUI: show modal "Enter the code Telegram sent you"
     User submits → TelegramAuthInput(otp)
  └─ [if 2FA] → send AuthPasswordPrompt(Telegram)
     TUI: show modal "Enter your 2FA password" (masked input)
     User submits → TelegramAuthInput(password)
  └─ Success → AuthStatusChanged(Telegram, Authenticated) + save session
```

### TUI Auth Overlay

A new `TelegramAuthState` in `app_state.rs`:

```rust
pub struct TelegramAuthState {
    pub stage: TelegramAuthStage,  // Phone / Otp / Password
    pub input: TextArea,
}

pub enum TelegramAuthStage { Phone, Otp, Password }
```

The overlay renders as a centered modal (similar to the settings overlay) with a prompt string and a single-line input. `Enter` submits; `Esc` is not meaningful here (auth must complete). On submit, `TelegramAuthInput(value)` is sent back to the provider via `auth_tx`.

### Retry Logic

Wrong OTP or password → provider receives an error from `grammers` → re-sends the appropriate `AuthXxxPrompt` event with an error message. After 3 consecutive failures → `AuthStatusChanged(Telegram, Failed)` + status bar message. User can restart the app to retry.

### Session Persistence

`grammers` supports a `FileSession`. Session is saved to `{data_dir}/telegram-session.db` (or `.session` file, depending on `grammers` API). On subsequent launches, `start()` detects an existing session, skips auth, and goes directly to `AuthStatusChanged(Telegram, Authenticated)`.

---

## Data Mapping

### Chat ID format

`tg-{peer_id}` where `peer_id` is the Telegram numeric ID (i64). Examples:
- DM with user 123456789 → `tg-123456789`
- Group/channel → `tg--1001234567890` (Telegram uses negative IDs for groups/channels)

### `Dialog` → `UnifiedChat` (in `convert.rs`)

| grammers field | UnifiedChat field | Notes |
|---|---|---|
| `dialog.chat().id()` | `id` | prefixed with `tg-` |
| `dialog.chat().name()` | `name` | username or display name |
| — | `display_name` | `None` (user can rename via `r`) |
| `dialog.last_message` | `last_message` | text preview |
| `dialog.unread_count()` | `unread_count` | |
| `chat` is `Chat::Group` or `Chat::Channel` | `is_group` | true for both |
| `chat` is `Chat::Channel` with broadcast | `is_newsletter` | channels = newsletters |
| `dialog.pinned()` | `is_pinned` | |
| — | `is_muted` | `false` initially (grammers exposes this; can add later) |
| — | `platform` | `Platform::Telegram` |

### `Message` → `UnifiedMessage` (in `convert.rs`)

| grammers field | UnifiedMessage field | Notes |
|---|---|---|
| `msg.id().to_string()` | `id` | |
| `tg-{peer_id}` | `chat_id` | |
| `Platform::Telegram` | `platform` | |
| `msg.sender()` name | `sender` | sender display name |
| `msg.text()` | `content` | `MessageContent::Text(...)` |
| `msg.date()` | `timestamp` | converted to `DateTime<Utc>` |
| `MessageStatus::Sent` | `status` | default; updated via status events |
| `msg.outgoing()` | `is_outgoing` | |

### Update Loop

The spawned task calls `client.next_update().await` in a loop and maps:

| grammers update | ProviderEvent |
|---|---|
| `Update::NewMessage(msg)` | `ProviderEvent::NewMessage(...)` |
| `Update::MessageDeleted(...)` | ignored (v1) |
| `Update::MessageEdited(msg)` | `ProviderEvent::NewMessage(...)` (re-emit as new, simplest path) |
| `Update::ReadHistoryInbox { peer, .. }` | `ProviderEvent::SelfRead { chat_id }` |

---

## Provider Operations

### `send_message(chat_id, content)`

1. Parse `peer_id` from `tg-{peer_id}`
2. Resolve to a `grammers` `InputPeer` via `client.resolve_username()` or by caching the peer during dialog load
3. `client.send_message(peer, text).await`
4. Return a synthetic `UnifiedMessage` (same pattern as WhatsApp)

A `PeerCache: Arc<Mutex<HashMap<String, InputPeer>>>` is populated during `get_chats()` and reused for `send_message()`.

### `get_chats()`

Calls `client.iter_dialogs()`, collects up to N dialogs, converts each to `UnifiedChat`, populates `PeerCache`.

### `get_messages(chat_id)`

Calls `client.iter_messages(peer).limit(50)`, converts each to `UnifiedMessage`.

### `mark_as_read(chat_id, msg_ids)`

Calls `client.mark_as_read(peer)` (grammers marks the whole dialog read).

---

## Error Handling

| Scenario | Handling |
|---|---|
| `api_id = 0` or `api_hash` empty at startup | Log error, skip provider registration |
| Auth OTP wrong | Re-send `AuthOtpPrompt` with error hint, retry up to 3× |
| Auth 2FA password wrong | Re-send `AuthPasswordPrompt` with error hint, retry up to 3× |
| 3 consecutive auth failures | `AuthStatusChanged(Telegram, Failed)`, status bar message |
| Network error in update loop | Log + exponential backoff retry (1s, 2s, 4s), surface as `Failed` after 3 attempts |
| `send_message` failure | Log error (same as WhatsApp path) |
| Session file missing/corrupt | Delete and re-auth (treat as no session) |

---

## Testing

| Test | Type | Location |
|---|---|---|
| `TelegramConfig` TOML parsing | Unit | `settings.rs` `#[cfg(test)]` |
| `convert_message()` | Unit | `convert.rs` `#[cfg(test)]` |
| `convert_dialog()` | Unit | `convert.rs` `#[cfg(test)]` |
| Chat ID encode/decode round-trip | Unit | `convert.rs` |
| Auth flow (manual) | Manual | Enable in `default.toml`, run app |

No integration tests against the real Telegram network (impractical in CI). The `MockProvider` continues to serve as the primary integration-style test harness.

---

## Out of Scope (v1)

- Media messages (images, files, stickers) — mapped to `MessageContent::Text("[Media]")` with a note
- Reactions and edits (edits are re-emitted as new messages)
- Mute status sync from Telegram
- Contact/username search
- Message deletion
- Telegram channels requiring subscription
