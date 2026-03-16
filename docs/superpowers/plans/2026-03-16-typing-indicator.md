# Typing Indicator Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a pulsing green dot + "typing" label in the chat list when a Telegram or WhatsApp contact is typing, disappearing automatically after 5 seconds.

**Architecture:** A new `ProviderEvent::Typing` variant is fired by Telegram (via `Update::Raw` TL downcasting) and WhatsApp (via `Event::ChatPresence`). `AppState` holds a `HashMap<String, TypingInfo>` keyed by chat ID; expired entries are cleaned up on each 250ms tick. A `blink_phase: bool` (toggled every 2 ticks) drives the green↔gray animation. The chat list widget reads both fields to render the indicator inline with the chat name.

**Tech Stack:** Rust, ratatui 0.29, grammers (Telegram MTProto), whatsapp-rust, tokio

---

## Chunk 1: Core data model + event plumbing

### Task 1: Add `ProviderEvent::Typing` to the event enum

**Files:**
- Modify: `src/core/provider.rs:11-33`

- [ ] **Step 1.1: Add the new variant**

Open `src/core/provider.rs`. The `ProviderEvent` enum ends at line 33. Add the new variant before the closing `}`:

```rust
    /// A contact in the given chat is currently typing.
    /// Fires on each typing update from the platform; the TUI expires the
    /// indicator automatically after 5 seconds without a new event.
    Typing { chat_id: String, user_name: String },
```

The full enum after the change:
```rust
#[derive(Debug, Clone)]
pub enum ProviderEvent {
    NewMessage(UnifiedMessage),
    MessageUpdated(UnifiedMessage),
    MessageStatusUpdate {
        message_id: String,
        status: MessageStatus,
    },
    ChatsUpdated(Vec<UnifiedChat>),
    AuthStatusChanged(Platform, AuthStatus),
    AuthQrCode(String),
    SyncCompleted,
    SelfRead { chat_id: String },
    AuthPhonePrompt(Platform, Option<String>),
    AuthOtpPrompt(Platform, Option<String>),
    AuthPasswordPrompt(Platform, Option<String>),
    LidPnMappingDiscovered { lid: String, pn: String },
    /// A contact in the given chat is currently typing.
    Typing { chat_id: String, user_name: String },
}
```

- [ ] **Step 1.2: Verify it compiles**

```bash
cargo check 2>&1 | head -30
```

Expected: warning about non-exhaustive match patterns in `src/app.rs` (the `handle_tick` event dispatch doesn't handle `Typing` yet). No errors beyond that.

- [ ] **Step 1.3: Commit**

```bash
git add src/core/provider.rs
git commit -m "feat: add ProviderEvent::Typing variant for typing indicators"
```

---

### Task 2: Add `TypingInfo` and new fields to `AppState`

**Files:**
- Modify: `src/tui/app_state.rs:1-6` (imports), `317-347` (struct), `354-381` (`new()`)

- [ ] **Step 2.1: Add `Instant` and `HashMap` imports**

At the top of `src/tui/app_state.rs`, the current imports are:
```rust
use ratatui::widgets::ListState;
use tui_textarea::TextArea;

use crate::config::AppConfig;
use crate::core::types::{Platform, UnifiedChat, UnifiedMessage};
use crate::storage::ScheduledMessage;
```

Change to:
```rust
use std::collections::HashMap;
use std::time::Instant;

use ratatui::widgets::ListState;
use tui_textarea::TextArea;

use crate::config::AppConfig;
use crate::core::types::{Platform, UnifiedChat, UnifiedMessage};
use crate::storage::ScheduledMessage;
```

- [ ] **Step 2.2: Add `TypingInfo` struct**

After the last `use` statement and before the first `#[derive]` line in the file (line 8, the `InputMode` enum), insert:

```rust
/// Tracks a contact who is currently typing in a chat.
#[derive(Debug, Clone)]
pub struct TypingInfo {
    pub user_name: String,
    pub expires_at: Instant,
}
```

- [ ] **Step 2.3: Add fields to `AppState` struct**

At the end of the `AppState` struct (after `pub schedule_status: Option<String>`), add:

```rust
    /// Per-chat typing indicators: chat_id → who is typing and when it expires.
    pub typing_states: HashMap<String, TypingInfo>,
    /// Toggled every 2 ticks (~500ms) to drive the green↔gray blink animation.
    pub blink_phase: bool,
```

- [ ] **Step 2.4: Initialize in `AppState::new()`**

In the `Self { ... }` block of `AppState::new()`, after `schedule_status: None,` add:

```rust
            typing_states: HashMap::new(),
            blink_phase: false,
```

- [ ] **Step 2.5: Write a unit test**

At the end of `src/tui/app_state.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_app_state_new_has_empty_typing_states() {
        let state = AppState::new();
        assert!(state.typing_states.is_empty());
        assert!(!state.blink_phase);
    }

    #[test]
    fn test_typing_info_expiry() {
        let expired = TypingInfo {
            user_name: "Alice".to_string(),
            expires_at: Instant::now() - Duration::from_secs(1),
        };
        let active = TypingInfo {
            user_name: "Bob".to_string(),
            expires_at: Instant::now() + Duration::from_secs(5),
        };
        let now = Instant::now();
        assert!(expired.expires_at <= now, "expired entry should be in the past");
        assert!(active.expires_at > now, "active entry should be in the future");
    }
}
```

- [ ] **Step 2.6: Run the tests**

```bash
cargo test tui::app_state::tests -- --nocapture
```

Expected: both tests pass.

- [ ] **Step 2.7: Commit**

```bash
git add src/tui/app_state.rs
git commit -m "feat: add TypingInfo struct and typing_states/blink_phase to AppState"
```

---

### Task 3: Wire tick handler and event dispatch in `App`

**Files:**
- Modify: `src/app.rs:34-48` (struct), `84-98` (`new()`), `337-351` (`handle_tick`), `648-651` (event match end)

- [ ] **Step 3.1: Add `tick_count` field to `App` struct**

In `src/app.rs`, the `App` struct (lines 34–48) currently ends with `event_tx`. Add `tick_count` before the closing `}`:

```rust
pub struct App {
    state: AppState,
    router: MessageRouter,
    db: Database,
    address_book: AddressBook,
    config: AppConfig,
    config_path: PathBuf,
    ai_worker: Option<AiWorker>,
    last_keystroke: Option<Instant>,
    db_summary_tx: tokio::sync::mpsc::UnboundedSender<(String, String)>,
    db_summary_rx: tokio::sync::mpsc::UnboundedReceiver<(String, String)>,
    schedule_status_ticks: u8,
    tick_count: u64,    // <-- new
    telegram_auth_tx: Option<tokio::sync::mpsc::UnboundedSender<crate::providers::telegram::AuthInput>>,
    event_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
}
```

- [ ] **Step 3.2: Initialize `tick_count` in `App::new()`**

In the `Self { ... }` block of `App::new()`, after `schedule_status_ticks: 0,` add:

```rust
            tick_count: 0,
```

- [ ] **Step 3.3: Add required imports to `app.rs`**

At the top of `src/app.rs`, find the existing imports and verify `Duration` and `TypingInfo` are available. Add to the `use crate::tui::app_state` import line:

```rust
use crate::tui::app_state::{AppState, InputMode, TypingInfo};
```

Also confirm `std::time::{Duration, Instant}` is already imported (it is — check for `Instant` usage). If `Duration` is not imported, add it.

- [ ] **Step 3.4: Add tick_count increment and typing expiry to `handle_tick`**

The `handle_tick` function starts at line 337. After the `schedule_status_ticks` block and before `let events = self.router.poll_events();`, add:

```rust
        // Advance tick counter and drive typing indicator animation
        self.tick_count += 1;
        let now = std::time::Instant::now();
        self.state.typing_states.retain(|_, v| v.expires_at > now);
        if self.tick_count % 2 == 0 {
            self.state.blink_phase = !self.state.blink_phase;
        }
```

- [ ] **Step 3.5: Handle `ProviderEvent::Typing` in the event dispatch**

The event dispatch match in `handle_tick` ends at line 648–651:
```rust
                ProviderEvent::LidPnMappingDiscovered { lid, pn } => {
                    ...
                }
            }   // ← end of match event
        }       // ← end of for event in events
    }           // ← end of handle_tick
```

Before the final `}` of the `match event` block (after the `LidPnMappingDiscovered` arm), add:

```rust
                ProviderEvent::Typing { chat_id, user_name } => {
                    self.state.typing_states.insert(chat_id, TypingInfo {
                        user_name,
                        expires_at: std::time::Instant::now() + std::time::Duration::from_secs(5),
                    });
                }
```

- [ ] **Step 3.6: Verify it compiles**

```bash
cargo check 2>&1 | head -30
```

Expected: clean compile (no errors). There may be warnings about unused imports — those are fine for now.

- [ ] **Step 3.7: Commit**

```bash
git add src/app.rs
git commit -m "feat: add tick_count to App, expire typing indicators on tick, handle ProviderEvent::Typing"
```

---

## Chunk 2: Platform provider implementations

### Task 4: Telegram — handle typing updates via `Update::Raw`

**Files:**
- Modify: `src/providers/telegram/mod.rs:481-540`

**Context:** The update loop currently lives inside `match update_stream.next().await { Ok(update) => { ... } }` (lines 482–534). The entire `Ok(update) => { ... }` arm must be restructured: add an `if let Update::Raw` guard before the existing message-binding `match`.

- [ ] **Step 4.1: Replace the `Ok(update) =>` arm**

Find lines 483–534 in `src/providers/telegram/mod.rs`. Replace the entire `Ok(update) => { ... }` block with:

```rust
                Ok(update) => {
                    // Handle Raw typing updates first (borrow avoids moving `update`).
                    // The `continue` skips the message-processing path below.
                    if let grammers_client::update::Update::Raw(raw) = &update {
                        match &raw.raw {
                            tl::enums::Update::UserTyping(u) => {
                                // Private chat: chat_id derived from the peer user id
                                let chat_id = peer_id_to_chat_id(u.user_id);
                                let user_name = chat_name_cache
                                    .get(&chat_id)
                                    .cloned()
                                    .unwrap_or_else(|| format!("User {}", u.user_id));
                                let _ = tx.send(ProviderEvent::Typing { chat_id, user_name });
                            }
                            tl::enums::Update::ChatUserTyping(u) => {
                                // Legacy group: participant name resolution not attempted;
                                // chat_name_cache is keyed by chat id, not participant id.
                                let chat_id = peer_id_to_chat_id(u.chat_id);
                                let _ = tx.send(ProviderEvent::Typing {
                                    chat_id,
                                    user_name: "someone".to_string(),
                                });
                            }
                            tl::enums::Update::ChannelUserTyping(u) => {
                                let chat_id = peer_id_to_chat_id(u.channel_id);
                                let _ = tx.send(ProviderEvent::Typing {
                                    chat_id,
                                    user_name: "someone".to_string(),
                                });
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Message path (unchanged) — only NewMessage and MessageEdited reach here.
                    let (msg, is_edit) = match update {
                        grammers_client::update::Update::NewMessage(m) => (m, false),
                        grammers_client::update::Update::MessageEdited(m) => (m, true),
                        _ => continue,
                    };

                    let chat_id = msg
                        .peer_id()
                        .bot_api_dialog_id()
                        .map(peer_id_to_chat_id)
                        .unwrap_or_else(|| "tg-unknown".to_string());

                    // Re-fetch the message by ID to get the full text.
                    let full_msg: Option<grammers_client::message::Message> =
                        if let Some(peer_ref) = peer_cache.get(&chat_id) {
                            match client.get_messages_by_id(peer_ref, &[msg.id()]).await {
                                Ok(mut msgs) => msgs.pop().flatten(),
                                Err(e) => {
                                    tracing::warn!(
                                        "get_messages_by_id failed for msg {}: {}; using update payload",
                                        msg.id(), e
                                    );
                                    None
                                }
                            }
                        } else {
                            None
                        };

                    let effective_msg: &grammers_client::message::Message =
                        full_msg.as_ref().unwrap_or(&msg);

                    let fallback = chat_name_cache.get(&chat_id);
                    if let Some(unified) = grammers_message_to_unified(effective_msg, &chat_id, fallback.as_deref()) {
                        tracing::debug!(
                            chat_id = %chat_id,
                            msg_id = %effective_msg.id(),
                            sender = %unified.sender,
                            is_edit = %is_edit,
                            raw_text = %effective_msg.text(),
                            "telegram raw message (live update)"
                        );
                        if is_edit {
                            let _ = tx.send(ProviderEvent::MessageUpdated(unified));
                        } else {
                            let _ = tx.send(ProviderEvent::NewMessage(unified));
                        }
                    }
                }
```

- [ ] **Step 4.2: Verify it compiles**

```bash
cargo check 2>&1 | head -30
```

Expected: clean compile. The `tl::enums::Update::UserTyping` etc. are all real TL types in the grammers crate. If `tl` is not in scope, check the top of the file for `use grammers_tl_types as tl;` — it should already be there.

> **Troubleshooting:** If `tl::enums::Update::UserTyping` gives "no field `user_id`", the TL field names may differ slightly across grammers versions. Run `cargo doc --open` and search for `UpdateUserTyping` to confirm field names.

- [ ] **Step 4.3: Commit**

```bash
git add src/providers/telegram/mod.rs
git commit -m "feat: handle Telegram typing events via Update::Raw TL downcasting"
```

---

### Task 5: WhatsApp — handle `Event::ChatPresence`

**Files:**
- Modify: `src/providers/whatsapp/mod.rs:237-238` (imports in `handle_wa_event`), `428-434` (event match end)

- [ ] **Step 5.1: Add `ChatPresence` to the local imports inside `handle_wa_event`**

The function `handle_wa_event` at line 237–238 has local `use` statements:
```rust
    use whatsapp_rust::types::events::Event;
    use whatsapp_rust::Jid;
```

Add `ChatPresence` to the block:
```rust
    use whatsapp_rust::types::events::{ChatPresence, Event};
    use whatsapp_rust::Jid;
```

- [ ] **Step 5.2: Add the `ChatPresence` match arm**

The current match ends at lines 428–435:
```rust
        Event::OfflineSyncCompleted(_) => {
            tracing::info!("WhatsApp offline sync completed");
            let _ = tx.send(ProviderEvent::SyncCompleted);
        }
        _ => {
            tracing::trace!("Unhandled WhatsApp event");
        }
    }
```

Before the wildcard `_ =>` arm, insert:
```rust
        Event::ChatPresence(update) => {
            if update.state == ChatPresence::Composing {
                let chat_id = format!("wa-{}", update.source.chat);
                // Use source.sender (not source.chat) — they differ in group chats
                // where source.chat is the group JID and source.sender is the typer.
                let user_name = update.source.sender.user.clone();
                tracing::debug!(chat_id = %chat_id, user_name = %user_name, "WhatsApp typing");
                let _ = tx.send(ProviderEvent::Typing { chat_id, user_name });
            }
        }
```

- [ ] **Step 5.3: Verify it compiles**

```bash
cargo check 2>&1 | head -30
```

Expected: clean compile.

> **Troubleshooting:** If `ChatPresence` is not found at that path, run:
> ```bash
> cargo doc 2>/dev/null && grep -r "ChatPresence" target/doc/whatsapp_rust/ 2>/dev/null | head -5
> ```
> or search the whatsapp-rust source:
> ```bash
> grep -r "ChatPresence" ~/.cargo/git/checkouts/whatsapp-rust-*/*/wacore/src/ 2>/dev/null | head -10
> ```
> Adjust the import path accordingly.

> **Troubleshooting:** If `update.source.sender.user` doesn't exist, check the `MessageSource` type in the whatsapp-rust crate. The field names for the JID parts may differ (e.g. `.user_part()` method vs `.user` field).

- [ ] **Step 5.4: Commit**

```bash
git add src/providers/whatsapp/mod.rs
git commit -m "feat: handle WhatsApp ChatPresence typing events"
```

---

## Chunk 3: TUI rendering

### Task 6: Update chat list widget to show typing indicator

**Files:**
- Modify: `src/tui/widgets/chat_list.rs` (full file)
- Modify: `src/tui/render.rs:56-63` (call site)

**Context:** `make_item(chat, is_selected)` builds the `ListItem` for each chat row. We'll add an optional `typing_blink: Option<bool>` parameter — `None` = not typing, `Some(phase)` = show the pulsing dot. The `render_chat_list` function gets two new parameters.

- [ ] **Step 6.1: Add imports to `chat_list.rs`**

At the top of `src/tui/widgets/chat_list.rs`, the current imports end at line 10:
```rust
use crate::tui::app_state::{ActivePanel, InputMode};
```

Change to:
```rust
use std::collections::HashMap;

use crate::tui::app_state::{ActivePanel, InputMode, TypingInfo};
```

- [ ] **Step 6.2: Update `make_item` signature and body**

Change the `make_item` signature from:
```rust
fn make_item(chat: &UnifiedChat, is_selected: bool) -> ListItem<'static> {
```
to:
```rust
fn make_item(chat: &UnifiedChat, is_selected: bool, typing_blink: Option<bool>) -> ListItem<'static> {
```

Inside `make_item`, find the `spans` vec that currently ends with:
```rust
    let spans = vec![
        Span::raw(selector),
        Span::styled(pin_tag.to_string(), Style::default().fg(Color::Yellow)),
        platform_span,
        emoji_span,
        Span::styled(name, Style::default().fg(name_color)),
        Span::styled(unread, Style::default().fg(unread_color)),
    ];
```

Replace with:
```rust
    let spans = if let Some(phase) = typing_blink {
        let dot_color = if phase { Color::Green } else { Color::DarkGray };
        vec![
            Span::raw(selector),
            Span::styled(pin_tag.to_string(), Style::default().fg(Color::Yellow)),
            platform_span,
            emoji_span,
            Span::styled("● ", Style::default().fg(dot_color)),
            Span::styled(name, Style::default().fg(name_color)),
            Span::styled(" typing", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        vec![
            Span::raw(selector),
            Span::styled(pin_tag.to_string(), Style::default().fg(Color::Yellow)),
            platform_span,
            emoji_span,
            Span::styled(name, Style::default().fg(name_color)),
            Span::styled(unread, Style::default().fg(unread_color)),
        ]
    };
```

- [ ] **Step 6.3: Update `render_chat_list` signature**

Change the function signature from:
```rust
pub fn render_chat_list(
    f: &mut Frame,
    area: Rect,
    chats: &[UnifiedChat],
    list_state: &mut ListState,
    active_panel: ActivePanel,
    input_mode: InputMode,
) {
```
to:
```rust
pub fn render_chat_list(
    f: &mut Frame,
    area: Rect,
    chats: &[UnifiedChat],
    list_state: &mut ListState,
    active_panel: ActivePanel,
    input_mode: InputMode,
    typing_states: &HashMap<String, TypingInfo>,
    blink_phase: bool,
) {
```

- [ ] **Step 6.4: Thread typing info into `make_item` call sites**

There are three places in `render_chat_list` that call `make_item`. Update each one:

**Site 1** — no-pinned-chats path (line ~121):
```rust
        let items: Vec<ListItem> = chats
            .iter()
            .enumerate()
            .map(|(i, chat)| make_item(chat, i == selected))
            .collect();
```
→
```rust
        let items: Vec<ListItem> = chats
            .iter()
            .enumerate()
            .map(|(i, chat)| {
                let blink = typing_states.get(&chat.id).map(|_| blink_phase);
                make_item(chat, i == selected, blink)
            })
            .collect();
```

**Site 2** — pinned section (line ~140):
```rust
    let pinned_items: Vec<ListItem> = chats
        .iter()
        .filter(|c| c.is_pinned)
        .enumerate()
        .map(|(i, chat)| make_item(chat, selected < pinned_count && i == selected))
        .collect();
```
→
```rust
    let pinned_items: Vec<ListItem> = chats
        .iter()
        .filter(|c| c.is_pinned)
        .enumerate()
        .map(|(i, chat)| {
            let blink = typing_states.get(&chat.id).map(|_| blink_phase);
            make_item(chat, selected < pinned_count && i == selected, blink)
        })
        .collect();
```

**Site 3** — unpinned section (line ~155):
```rust
    let unpinned_items: Vec<ListItem> = chats
        .iter()
        .filter(|c| !c.is_pinned)
        .enumerate()
        .map(|(i, chat)| {
            make_item(
                chat,
                selected >= pinned_count && i == selected - pinned_count,
            )
        })
        .collect();
```
→
```rust
    let unpinned_items: Vec<ListItem> = chats
        .iter()
        .filter(|c| !c.is_pinned)
        .enumerate()
        .map(|(i, chat)| {
            let blink = typing_states.get(&chat.id).map(|_| blink_phase);
            make_item(
                chat,
                selected >= pinned_count && i == selected - pinned_count,
                blink,
            )
        })
        .collect();
```

- [ ] **Step 6.5: Update the call site in `render.rs`**

In `src/tui/render.rs`, the call at lines 56–63:
```rust
    chat_list::render_chat_list(
        f,
        chat_list_area,
        &state.chats,
        &mut state.chat_list_state,
        state.active_panel,
        state.input_mode,
    );
```
→
```rust
    chat_list::render_chat_list(
        f,
        chat_list_area,
        &state.chats,
        &mut state.chat_list_state,
        state.active_panel,
        state.input_mode,
        &state.typing_states,
        state.blink_phase,
    );
```

- [ ] **Step 6.6: Verify full compile**

```bash
cargo check 2>&1
```

Expected: zero errors.

- [ ] **Step 6.7: Run all tests**

```bash
cargo test 2>&1 | tail -20
```

Expected: all tests pass (no regressions).

- [ ] **Step 6.8: Commit**

```bash
git add src/tui/widgets/chat_list.rs src/tui/render.rs
git commit -m "feat: render typing indicator in chat list with pulsing green dot"
```

---

## Final verification

- [ ] **Step F.1: Full build**

```bash
cargo build 2>&1 | tail -10
```

Expected: compiles with zero errors.

- [ ] **Step F.2: Smoke test — Telegram typing**

1. Run the app: `cargo run`
2. Open a Telegram chat with a contact
3. Ask the contact to type something (or type on another device in the same chat)
4. Observe: a pulsing `●` dot appears next to the chat name in the left panel within a second
5. Stop typing: indicator disappears within 5 seconds

- [ ] **Step F.3: Smoke test — WhatsApp typing**

1. Open a WhatsApp chat with a contact
2. Have the contact start typing
3. Observe: same pulsing dot + "typing" label in chat list
4. Stop typing: disappears within 5 seconds

- [ ] **Step F.4: Final commit**

```bash
git add -p  # review any uncommitted changes
git commit -m "chore: typing indicator feature complete"
```
