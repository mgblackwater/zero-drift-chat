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

New struct and fields added to `tui/app_state.rs`:

```rust
pub struct TypingInfo {
    pub user_name: String,
    pub expires_at: Instant,
}
```

New fields on `AppState`:

```rust
pub typing_states: HashMap<ChatId, TypingInfo>,
pub blink_phase: bool,
```

- `typing_states`: keyed by `ChatId`, holds the typer's name and expiry timestamp
- `blink_phase`: toggled every 2 ticks (~500ms) to drive the green↔gray pulse

### 2. ProviderEvent

One new variant in `core/provider.rs`:

```rust
pub enum ProviderEvent {
    // ... existing variants unchanged ...
    Typing { chat_id: ChatId, user_name: String },
}
```

No changes to the `MessagingProvider` trait or `MessageRouter`.

### 3. Telegram Provider

File: `src/providers/telegram/mod.rs`

In the existing `stream_updates()` loop, add a match arm:

```rust
Update::UserTyping(typing) => {
    let chat_id = /* derive from typing.peer */;
    let user_name = /* look up from chat_name_cache */;
    let _ = event_tx.send(ProviderEvent::Typing { chat_id, user_name });
}
```

Grammers exposes both `updateUserTyping` (private chats) and `updateChatUserTyping` (groups) via `Update::UserTyping`. The `peer` field identifies the chat; `chat_name_cache` (already in scope) provides the display name.

### 4. WhatsApp Provider

File: `src/providers/whatsapp/mod.rs`

Handle the typing presence event from `whatsapp-rust` (exact event name to be confirmed during implementation), extract `chat_id` and sender display name, send `ProviderEvent::Typing`.

### 5. AppState — Tick Handler

File: `src/app.rs`

**On `AppEvent::Tick`:**

```rust
// 1. Expire stale indicators
let now = Instant::now();
self.state.typing_states.retain(|_, v| v.expires_at > now);

// 2. Advance blink phase every 2 ticks (~500ms)
if self.tick_count % 2 == 0 {
    self.state.blink_phase = !self.state.blink_phase;
}
```

`tick_count` is the existing field already used for AI debounce — no new field needed.

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

When rendering each chat row, check `typing_states`:

```rust
if let Some(typing) = app_state.typing_states.get(&chat.id) {
    let dot_color = if app_state.blink_phase {
        Color::Green
    } else {
        Color::DarkGray
    };
    line.push(Span::styled("● ", Style::default().fg(dot_color)));
    line.push(Span::styled(&chat.name, name_style));
    line.push(Span::styled(" typing", Style::default().fg(Color::DarkGray)));
} else {
    // existing render path
}
```

The dot pulses green↔gray. The "typing" label is always dim gray. The chat name retains its normal style (highlighted when selected).

## Data Flow

```
Platform event (Telegram/WhatsApp)
  → Provider fires ProviderEvent::Typing { chat_id, user_name }
  → MessageRouter delivers to AppState
  → typing_states.insert(chat_id, TypingInfo { expires_at: now + 5s })

AppEvent::Tick (every 250ms)
  → typing_states.retain(|_, v| v.expires_at > now)   // expire old
  → blink_phase toggled every 2 ticks                  // drive animation

AppEvent::Render (every 33ms)
  → chat_list widget reads typing_states + blink_phase
  → renders ● (green or gray) + "typing" label inline
```

## Files Changed

| File | Change |
|------|--------|
| `src/core/provider.rs` | Add `ProviderEvent::Typing` variant |
| `src/tui/app_state.rs` | Add `TypingInfo` struct, `typing_states` and `blink_phase` fields |
| `src/app.rs` | Handle `ProviderEvent::Typing`; expire + blink logic on tick |
| `src/providers/telegram/mod.rs` | Handle `Update::UserTyping` in update loop |
| `src/providers/whatsapp/mod.rs` | Handle typing presence event |
| `src/tui/widgets/chat_list.rs` | Render typing indicator in chat rows |

## Out of Scope

- Mock provider typing simulation (can be added later for testing)
- Group chat: multiple people typing simultaneously (show first typer only for now)
- "Stopped typing" explicit event (timeout handles this)
- Indicator in the message view area (placement decision: chat list only)
