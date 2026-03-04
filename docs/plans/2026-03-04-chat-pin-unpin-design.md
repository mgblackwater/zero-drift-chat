# Design: Chat-Level Context Menu with Pin/Unpin (Issue #4)

Date: 2026-03-04

## Overview

Add a per-chat context menu triggered by `x` in Normal mode. Initially exposes a single action — pin/unpin — but is architected for future extension (mute, archive, delete, etc.). Pinned chats float to the top of the chat list persistently across sessions.

## Approach

Option A: New `InputMode::ChatMenu` — a small popup overlay on the chat list, consistent with the existing `Settings` overlay pattern.

## Section 1: Data & Storage

- Add `is_pinned: bool` to `UnifiedChat` struct in `src/core/types.rs`
- Add `pinned INTEGER NOT NULL DEFAULT 0` column to `chats` SQLite table via migration in `db.rs` (same pattern as `display_name`)
- New DB method `Database::set_chat_pinned(chat_id: &str, pinned: bool)` in `storage/chats.rs`
- `get_all_chats()` query updated to `ORDER BY pinned DESC, updated_at DESC` so pinned chats always appear at top
- `upsert_chat()` preserves existing `pinned` value using `COALESCE` (won't overwrite user-set pin on sync)

## Section 2: State & Mode

New types in `src/tui/app_state.rs`:

```rust
pub enum ChatMenuItem {
    TogglePin,
    // future: Mute, Archive, Delete
}

pub struct ChatMenuState {
    pub chat_id: String,
    pub chat_name: String,
    pub is_pinned: bool,
    pub selected: usize,
    pub items: Vec<ChatMenuItem>,
}
```

- `InputMode::ChatMenu` added to the `InputMode` enum
- `AppState` gets `pub chat_menu_state: Option<ChatMenuState>`
- New `Action` variants: `OpenChatMenu`, `ChatMenuNext`, `ChatMenuPrev`, `ChatMenuConfirm`, `ChatMenuClose`
- New `map_chat_menu_mode()` in `keybindings.rs`:
  - `j` / `Down` → `ChatMenuNext`
  - `k` / `Up` → `ChatMenuPrev`
  - `Enter` / `p` → `ChatMenuConfirm`
  - `Esc` / `q` → `ChatMenuClose`
- `x` in `map_normal_mode()` → `OpenChatMenu`

## Section 3: Rendering

- New widget `src/tui/widgets/chat_menu.rs` — `render_chat_menu(f, area, state)`
- Popup: centered over the chat list, ~30 chars wide, height = `items.len() + 4` (borders + header)
- Header line shows chat name
- Menu items rendered as a list; selected item highlighted
- Item label is dynamic: `Pin` or `Unpin` based on `is_pinned`
- Pinned chats in the chat list show a `*` prefix (ASCII-safe) before the platform tag
- `render.rs` calls `render_chat_menu()` last (on top) when `input_mode == ChatMenu`

## Keybindings Summary

| Key | Context | Action |
|-----|---------|--------|
| `x` | Normal mode, chat list | Open chat context menu |
| `j` / `k` | Chat menu | Navigate items |
| `p` / `Enter` | Chat menu | Confirm selected action |
| `Esc` / `q` | Chat menu | Close menu |

## Extension Points

`ChatMenuItem` enum is the single place to add future actions. Each new variant requires:
1. A label in the render widget
2. A handler branch in `app.rs`
