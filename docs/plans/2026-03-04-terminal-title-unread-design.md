# Terminal Tab Title Unread Indicator — Design

## Goal

Show a dot prefix in the terminal tab title when there are unread messages, so Windows Terminal (WSL) gives a passive notification without any in-app overlay.

## Title Values

| State | Title |
|---|---|
| No unread messages | `zero-drift-chat` |
One or more unread | `● zero-drift-chat` |

On clean exit the title resets to `zero-drift-chat` — no dot left in the tab.

## Architecture

A single helper `fn update_title(has_unread: bool)` in `src/app.rs` emits the crossterm `SetTitle` command to stdout. No new field on `AppState`.

Called from three places:

1. **Startup** — after chats are loaded, compute any unread and set initial title.
2. **New incoming message** — inside `handle_tick` after `chat.unread_count += 1`, call `update_title(true)`.
3. **Chat selected** — after zeroing unread on the selected chat, call `update_title(self.state.chats.iter().any(|c| c.unread_count > 0))`.
4. **Exit** — before `LeaveAlternateScreen`, call `update_title(false)` to restore clean title.

## Constraints

- crossterm `SetTitle` is already available via the existing `crossterm` dependency.
- Works in Windows Terminal (WSL) via OSC escape sequence `\x1b]0;{title}\x07`.
- No config toggle needed — always on.
