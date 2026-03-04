# Input Improvements Design

**Date:** 2026-03-04
**Issues:** [#5 Input box scrolling bug](https://github.com/mgblackwater/zero-drift-chat/issues/5), [#2 Keyboard navigation & multi-line input](https://github.com/mgblackwater/zero-drift-chat/issues/2)

## Problem

1. **Bug #5:** Text typed beyond the visible width of the input box is not shown — no horizontal scroll, no cursor tracking past the edge.
2. **Issue #2 (priority scope):** No arrow-key cursor movement, no multi-line input, Enter always sends immediately.

## Decision

Replace the custom input box with **`tui-textarea`** — a ratatui-compatible widget that handles cursor movement, multi-line editing, horizontal/vertical scroll, Unicode boundaries, and Home/End out of the box. This eliminates ~150 lines of custom cursor/scroll logic in exchange for ~20 lines of integration code.

**Rejected alternatives:**
- Extend the String buffer with manual scroll offset: more code, more edge cases (Unicode, wrap, multi-line)
- `Vec<String>` per-line cursor: explicit but equally complex, still needs all the same scroll logic

## Keybindings

| Key | Action | Platform notes |
|---|---|---|
| `Enter` | Insert newline | All |
| `Shift+Enter` | Send message | Windows Terminal, iTerm2, modern macOS terminals, WSL |
| `Alt+Enter` | Send message | macOS Terminal.app fallback; works on all mainstream terminals |
| `Ctrl+U` | Clear entire input buffer | All (custom override — tui-textarea default is delete-to-line-start) |
| `← →` | Move cursor left/right | All |
| `↑ ↓` | Move cursor up/down (multi-line) | All |
| `Home` / `End` | Start / end of line | All (macOS: Fn+← / Fn+→) |
| `Backspace` | Delete character | All |
| all other keys | Forwarded to TextArea natively | — |

Both `Shift+Enter` and `Alt+Enter` fire the same `SubmitMessage` action.

## Architecture

### Dependency

```toml
# Cargo.toml
tui-textarea = "0.7"
```

### State (`src/tui/app_state.rs`)

Remove:
- `pub input_buffer: String`
- `pub cursor_position: usize`
- `push_char()`, `delete_char()`

Add:
- `pub input: TextArea<'static>`

Update:
- `take_input()` → returns `self.input.lines().join("\n")` and resets `self.input = TextArea::default()`
- New action handler for `ClearInput` → `self.input = TextArea::default()`

### Key handling (`src/tui/keybindings.rs` + `src/app.rs`)

In editing mode, intercept before forwarding to TextArea:
- `Shift+Enter` or `Alt+Enter` → `AppAction::SubmitMessage`
- `Ctrl+U` → `AppAction::ClearInput`
- `Esc` → `AppAction::ExitEditing`
- everything else → `state.input.input(key_event)` (TextArea handles it, returns bool indicating whether text changed)

Remove `AppAction::InsertChar` and `AppAction::DeleteChar`.

### Rendering (`src/tui/widgets/input_bar.rs`)

Replace:
```rust
// Before: manual Paragraph + set_cursor_position
let paragraph = Paragraph::new(text)...;
f.render_widget(paragraph, inner);
f.set_cursor_position((x, y));
```

With:
```rust
// After: TextArea widget (handles cursor, scroll, multi-line automatically)
f.render_widget(state.input.widget(), inner);
```

The input bar block/border stays as-is; only the inner widget changes.

## Scope

Four files changed:
- `Cargo.toml` — add dependency
- `src/tui/app_state.rs` — replace input state
- `src/tui/keybindings.rs` — update editing-mode key map
- `src/tui/widgets/input_bar.rs` — update render

No changes to chat list, message view, or any other component.
