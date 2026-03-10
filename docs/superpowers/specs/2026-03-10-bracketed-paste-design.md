# Bracketed Paste Support

**Date:** 2026-03-10
**Issue:** [#14 — pasting multi-line text sends each line as a separate message](https://github.com/mgblackwater/zero-drift-chat/issues/14)

## Problem

When `enter_sends = true` (default), pasting multi-line text into the chat input sends each line as a separate message. The terminal in raw mode emits pasted newlines as plain `Enter` key events, which the key handler maps to `SubmitMessage`.

## Root Cause

`src/tui/event.rs` only forwards `Event::Key` and `Event::Resize` events; `Event::Paste` falls through to `_ => {}` and is discarded. No bracketed paste mode is enabled, so the terminal never signals the start/end of a paste sequence.

## Design

Enable bracketed paste mode so the terminal wraps pasted content in escape sequences. Crossterm exposes this as `Event::Paste(String)`. Capture that event and insert the text directly into the `TextArea`, bypassing the Enter key handler entirely.

### Changes

| File | Change |
|------|--------|
| `src/app.rs` | Enable `EnableBracketedPaste` on startup; disable in cleanup and panic hook |
| `src/tui/event.rs` | Add `Paste(String)` variant to `AppEvent` |
| `src/tui/event.rs` | Match `Event::Paste(text)` in the event loop, send `AppEvent::Paste(text)` |
| `src/app.rs` | Handle `AppEvent::Paste(text)` — call `input.insert_str(&text)` when in editing mode |

### Paste handler behaviour

```rust
AppEvent::Paste(text) => {
    if self.state.input_mode == InputMode::Editing {
        self.state.input.insert_str(&text);
        self.state.ai_suggestion = None;
        if self.ai_worker.is_some() {
            self.last_keystroke = Some(Instant::now());
        }
    }
}
```

### Non-changes

- `keybindings.rs` — untouched; typed Enter still submits
- `tui-textarea` — no changes; `insert_str` is a stable API
- No new dependencies

## Testing

- Paste multi-line text → full block appears in input, not sent
- Type Enter after paste → message sent as one unit
- Paste in Normal mode → ignored (no crash)
- Ctrl+S after paste → submits as expected
- `enter_sends = false` mode → same paste behaviour, submit via Shift+Enter
