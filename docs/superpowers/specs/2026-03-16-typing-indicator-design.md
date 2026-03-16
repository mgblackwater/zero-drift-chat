# Typing Indicator — Design Spec

**Date:** 2026-03-16
**Status:** Approved

## Overview

Add a "typing indicator" to the chat list panel, similar to Telegram's "...typing" dynamic indicator. When a contact is typing in a chat, a pulsing green dot and "typing" label appear next to their chat entry in the left panel. The indicator disappears automatically 5 seconds after the last typing event.

## Requirements

- **Platforms:** Telegram and WhatsApp
- **Placement:** Chat list panel (left column), in-line with the chat name
- **Animation:** Green dot (●) pulses between bright green and dark gray at ~2 Hz
- **Timeout:** 5 seconds after the last received typing event
- **Multiple chats:** Each chat tracks its own typing state independently

## Design

### 1. Data Model

New struct added, and new fields on existing structs:

**`tui/app_state.rs`** — add `TypingInfo` struct and two fields on `AppState`:

```rust
pub struct TypingInfo {
    pub user_name: String,
    pub expires_at: Instant,
}

// in AppState:
pub typing_states: HashMap<String, TypingInfo>,  // keyed by chat id (String)
pub blink_phase: bool,
```

**`app.rs` (`App` struct)** — add one new field:

```rust
pub tick_count: u64,   // new field; incremented on every AppEvent::Tick
```

`blink_phase` lives on `AppState` (accessible to the render path).
`tick_count` lives on `App` (alongside `schedule_status_ticks`, which follows the same pattern).

Note: chat IDs throughout this codebase are plain `String` values (e.g. `UnifiedChat.id`, `UnifiedMessage.chat_id`). There is no `ChatId` newtype.

### 2. ProviderEvent

One new variant in `core/provider.rs`:

```rust
pub enum ProviderEvent {
    // ... existing variants unchanged ...
    Typing { chat_id: String, user_name: String },
}
```

No changes to the `MessagingProvider` trait or `MessageRouter`.

### 3. Telegram Provider

File: `src/providers/telegram/mod.rs`

The grammers library does **not** expose a first-class `Update::UserTyping` variant. Typing updates arrive via `Update::Raw(raw)`, where `raw.raw` is a `tl::enums::Update`. There are three relevant TL update types that must all be handled:

- `tl::enums::Update::UserTyping(u)` — private chats; `u.user_id` is the typer; chat ID is derived from the peer user
- `tl::enums::Update::ChatUserTyping(u)` — legacy groups; `u.chat_id` is the chat; `u.from_id` is the typer peer
- `tl::enums::Update::ChannelUserTyping(u)` — supergroups/channels; `u.channel_id` is the chat; `u.from_id` is the typer peer

**The existing update loop must be restructured.** The current loop uses a binding pattern that silently drops all non-message updates:

```rust
// BEFORE (current code — drops everything except NewMessage/MessageEdited):
let (msg, is_edit) = match update {
    Update::NewMessage(m) => (m, false),
    Update::MessageEdited(m) => (m, true),
    _ => continue,  // <-- this fires for Raw, dropping typing updates
};
```

This must be refactored so that `Update::Raw` is handled before the wildcard `continue`. The recommended restructure:

```rust
// AFTER — handle Raw typing events, then process message updates:
if let Update::Raw(raw) = &update {
    match &raw.raw {
        tl::enums::Update::UserTyping(u) => {
            let chat_id = format!("tg-{}", u.user_id);
            let user_name = chat_name_cache
                .get(&chat_id)
                .cloned()
                .unwrap_or_else(|| format!("User {}", u.user_id));
            let _ = event_tx.send(ProviderEvent::Typing { chat_id, user_name });
        }
        tl::enums::Update::ChatUserTyping(u) => {
            let chat_id = format!("tg-{}", u.chat_id);
            let _ = event_tx.send(ProviderEvent::Typing {
                chat_id,
                user_name: "someone".to_string(),
            });
        }
        tl::enums::Update::ChannelUserTyping(u) => {
            let chat_id = format!("tg-{}", u.channel_id);
            let _ = event_tx.send(ProviderEvent::Typing {
                chat_id,
                user_name: "someone".to_string(),
            });
        }
        _ => {}
    }
    continue;  // Raw updates are never messages; skip message-binding below
}

// existing message-binding path (unchanged):
let (msg, is_edit) = match update {
    Update::NewMessage(m) => (m, false),
    Update::MessageEdited(m) => (m, true),
    _ => continue,
};
```

**Note on group user names:** `chat_name_cache` is keyed by dialog/chat ID, not by participant user ID. For group typing events, resolving the participant's name would require a separate `client.get_entity()` call (async, could block the update loop) or a dedicated user name cache. For the initial implementation, group typers display as "someone" or a generic label. A user name cache can be layered on top later.

### 4. WhatsApp Provider

File: `src/providers/whatsapp/mod.rs`

The `whatsapp-rust` library emits `Event::ChatPresence(ChatPresenceUpdate)` for typing events. `ChatPresenceUpdate` has:
- `.state: ChatPresence` — `ChatPresence::Composing` (typing) or `ChatPresence::Paused` (stopped)
- `.source` — the sender JID (contains the chat JID and participant info)

In the existing `handle_wa_event` match block (already switches on `Event`), add:

```rust
Event::ChatPresence(update) => {
    if update.state == ChatPresence::Composing {
        let chat_id = format!("wa-{}", update.source.chat);
        // Use source.sender (not source.chat) — they differ in group chats where
        // source.chat is the group JID and source.sender is the typing participant.
        let user_name = update.source.sender.user.clone();
        let _ = event_tx.send(ProviderEvent::Typing { chat_id, user_name });
    }
}
```

### 5. AppState — Tick Handler

File: `src/app.rs`

**On `AppEvent::Tick`:**

```rust
self.tick_count += 1;

// 1. Expire stale indicators
let now = Instant::now();
self.state.typing_states.retain(|_, v| v.expires_at > now);

// 2. Advance blink phase every 2 ticks (~500ms)
if self.tick_count % 2 == 0 {
    self.state.blink_phase = !self.state.blink_phase;
}
```

**On `ProviderEvent::Typing`:**

```rust
ProviderEvent::Typing { chat_id, user_name } => {
    self.state.typing_states.insert(chat_id, TypingInfo {
        user_name,
        expires_at: Instant::now() + Duration::from_secs(5),
    });
}
```

Each new typing event resets the 5-second window, keeping the indicator alive during continuous typing.

### 6. Chat List Widget

File: `src/tui/widgets/chat_list.rs`

The current signature of `render_chat_list` does not include `AppState`. Two new parameters are added:

```rust
pub fn render_chat_list(
    f: &mut Frame,
    area: Rect,
    chats: &[UnifiedChat],
    list_state: &mut ListState,
    active_panel: ActivePanel,
    input_mode: InputMode,
    typing_states: &HashMap<String, TypingInfo>,  // new
    blink_phase: bool,                             // new
)
```

The call site in `src/tui/render.rs` is updated to pass `&app_state.typing_states` and `app_state.blink_phase`.

**Required `use` additions:**
- `src/tui/widgets/chat_list.rs`: add `use std::collections::HashMap;` and `use crate::tui::app_state::TypingInfo;`
- `src/tui/render.rs`: no new imports needed — `TypingInfo` is not referenced by name at the call site

When rendering each chat row:

```rust
if let Some(typing) = typing_states.get(&chat.id) {
    let dot_color = if blink_phase {
        Color::Green
    } else {
        Color::DarkGray
    };
    line.push(Span::styled("● ", Style::default().fg(dot_color)));
    line.push(Span::styled(&chat.name, name_style));
    line.push(Span::styled(" typing", Style::default().fg(Color::DarkGray)));
} else {
    // existing render path unchanged
}
```

The dot pulses green↔gray. The "typing" label stays dim gray. The chat name retains its normal style (highlighted when selected).

## Data Flow

```
Platform event (Telegram via Update::Raw TL typing / WhatsApp via Event::ChatPresence)
  → Provider fires ProviderEvent::Typing { chat_id: String, user_name: String }
  → MessageRouter delivers to AppState handler in app.rs
  → typing_states.insert(chat_id, TypingInfo { expires_at: now + 5s })

AppEvent::Tick (every 250ms)
  → tick_count += 1
  → typing_states.retain(|_, v| v.expires_at > now)   // expire stale entries
  → blink_phase toggled every 2 ticks (~500ms)          // drive animation

AppEvent::Render (every 33ms)
  → render_chat_list reads typing_states + blink_phase
  → renders ● (green or gray) + "typing" label inline per chat row
```

## Files Changed

| File | Change |
|------|--------|
| `src/core/provider.rs` | Add `ProviderEvent::Typing { chat_id: String, user_name: String }` variant |
| `src/tui/app_state.rs` | Add `TypingInfo` struct; add `typing_states: HashMap<String, TypingInfo>` and `blink_phase: bool` to `AppState` |
| `src/app.rs` | Add `tick_count: u64` to `App`; handle `ProviderEvent::Typing`; expire + blink logic on tick |
| `src/providers/telegram/mod.rs` | Match `Update::Raw` and downcast to three TL typing update types |
| `src/providers/whatsapp/mod.rs` | Handle `Event::ChatPresence` with `ChatPresence::Composing` state |
| `src/tui/widgets/chat_list.rs` | Add `typing_states` and `blink_phase` params; render indicator per chat row |
| `src/tui/render.rs` | Update `render_chat_list` call site to pass new parameters |

## Out of Scope

- Mock provider typing simulation (can be added later for testing)
- Group chat: multiple people typing simultaneously (show first typer only for now)
- Group chat participant name resolution (show generic label for initial implementation; user name cache can follow)
- "Stopped typing" explicit event (timeout handles this)
- Indicator in the message view area (placement decision: chat list only)
