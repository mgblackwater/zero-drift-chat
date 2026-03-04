# Chat Context Menu with Pin/Unpin Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a per-chat context menu (triggered by `x`) with a pin/unpin action that persists across sessions and floats pinned chats to the top of the list.

**Architecture:** New `InputMode::ChatMenu` overlay (mirrors existing `Settings` overlay pattern). Pin state stored in SQLite `chats.pinned` column. Chat list sorted `ORDER BY pinned DESC, updated_at DESC`. New `chat_menu.rs` widget renders a small popup over the chat list.

**Tech Stack:** Rust, ratatui 0.29, crossterm 0.28, rusqlite, tui-textarea 0.7

**Design doc:** `docs/plans/2026-03-04-chat-pin-unpin-design.md`

---

### Task 1: Add `is_pinned` to `UnifiedChat` and SQLite schema

**Files:**
- Modify: `src/core/types.rs`
- Modify: `src/storage/db.rs`

**Step 1: Add `is_pinned` field to `UnifiedChat`**

In `src/core/types.rs`, add `is_pinned` to the struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedChat {
    pub id: String,
    pub platform: Platform,
    pub name: String,
    pub display_name: Option<String>,
    pub last_message: Option<String>,
    pub unread_count: u32,
    pub is_group: bool,
    pub is_pinned: bool,   // NEW
}
```

**Step 2: Add migration for `pinned` column in `src/storage/db.rs`**

After the existing `display_name` migration (line ~68), add:

```rust
// Migration: add pinned column if not exists
let _ = self
    .conn
    .execute("ALTER TABLE chats ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0", []);
```

**Step 3: Build to verify no compile errors**

```bash
cargo build 2>&1 | grep "^error"
```

Expected: errors about missing `is_pinned` in struct initialisers — fix in next task.

**Step 4: Fix all struct initialisers that construct `UnifiedChat`**

Search for all `UnifiedChat {` usages:
```bash
grep -rn "UnifiedChat {" src/
```

Add `is_pinned: false` (or map from DB) to each. Key location is `src/storage/chats.rs` in `get_all_chats()` — map from `row.get(7)?` (add to SELECT).

**Step 5: Build to verify clean**

```bash
cargo build 2>&1 | grep "^error"
```

Expected: no errors.

**Step 6: Commit**

```bash
git add src/core/types.rs src/storage/db.rs src/storage/chats.rs
git commit -m "feat: add is_pinned field to UnifiedChat and SQLite migration"
```

---

### Task 2: Storage — `set_chat_pinned` and sorted query

**Files:**
- Modify: `src/storage/chats.rs`

**Step 1: Update `get_all_chats` to read `pinned` and sort**

Replace the SELECT query:

```rust
pub fn get_all_chats(&self) -> Result<Vec<UnifiedChat>> {
    let mut stmt = self.conn.prepare(
        "SELECT id, platform, name, last_message, unread_count, is_group, display_name, pinned
         FROM chats ORDER BY pinned DESC, updated_at DESC",
    )?;

    let chats = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let platform_str: String = row.get(1)?;
            let name: String = row.get(2)?;
            let last_message: Option<String> = row.get(3)?;
            let unread_count: u32 = row.get(4)?;
            let is_group: i32 = row.get(5)?;
            let display_name: Option<String> = row.get(6)?;
            let pinned: i32 = row.get(7)?;
            Ok((id, platform_str, name, last_message, unread_count, is_group, display_name, pinned))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let result = chats
        .into_iter()
        .map(|(id, platform_str, name, last_message, unread_count, is_group, display_name, pinned)| {
            let platform = match platform_str.as_str() {
                "WhatsApp" => Platform::WhatsApp,
                "Telegram" => Platform::Telegram,
                "Slack" => Platform::Slack,
                _ => Platform::Mock,
            };
            UnifiedChat {
                id,
                platform,
                name,
                display_name,
                last_message,
                unread_count,
                is_group: is_group != 0,
                is_pinned: pinned != 0,
            }
        })
        .collect();

    Ok(result)
}
```

**Step 2: Update `upsert_chat` to preserve `pinned`**

The existing `upsert_chat` must NOT overwrite the user-set pin. The `ON CONFLICT DO UPDATE` should not touch `pinned`. Current query already only updates specific columns — verify `pinned` is NOT in the update list. It should be fine since `pinned` isn't in the INSERT columns, so SQLite leaves it at DEFAULT 0 on insert and doesn't touch it on conflict update.

**Step 3: Add `set_chat_pinned` method**

```rust
pub fn set_chat_pinned(&self, chat_id: &str, pinned: bool) -> Result<()> {
    self.conn.execute(
        "UPDATE chats SET pinned = ?1 WHERE id = ?2",
        rusqlite::params![pinned as i32, chat_id],
    )?;
    Ok(())
}
```

**Step 4: Build clean**

```bash
cargo build 2>&1 | grep "^error"
```

**Step 5: Commit**

```bash
git add src/storage/chats.rs
git commit -m "feat: add set_chat_pinned and sort pinned chats to top"
```

---

### Task 3: State — `ChatMenuState` and `InputMode::ChatMenu`

**Files:**
- Modify: `src/tui/app_state.rs`

**Step 1: Add `ChatMenuItem` enum and `ChatMenuState` struct**

After the existing `SettingsState` block, add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatMenuItem {
    TogglePin,
}

impl ChatMenuItem {
    pub fn label(&self, is_pinned: bool) -> &'static str {
        match self {
            ChatMenuItem::TogglePin => if is_pinned { "Unpin" } else { "Pin" },
        }
    }
}

pub struct ChatMenuState {
    pub chat_id: String,
    pub chat_name: String,
    pub is_pinned: bool,
    pub selected: usize,
    pub items: Vec<ChatMenuItem>,
}

impl ChatMenuState {
    pub fn new(chat_id: String, chat_name: String, is_pinned: bool) -> Self {
        Self {
            chat_id,
            chat_name,
            is_pinned,
            selected: 0,
            items: vec![ChatMenuItem::TogglePin],
        }
    }

    pub fn select_next(&mut self) {
        if self.selected < self.items.len() - 1 {
            self.selected += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }
}
```

**Step 2: Add `ChatMenu` to `InputMode` enum**

```rust
pub enum InputMode {
    Normal,
    Editing,
    Settings,
    Renaming,
    ChatMenu,  // NEW
}
```

**Step 3: Add `chat_menu_state` field to `AppState`**

```rust
pub struct AppState {
    // ... existing fields ...
    pub chat_menu_state: Option<ChatMenuState>,  // NEW
}
```

In `AppState::new()`, add:
```rust
chat_menu_state: None,
```

**Step 4: Add `open_chat_menu` and `close_chat_menu` methods to `AppState`**

```rust
pub fn open_chat_menu(&mut self) {
    if let Some(idx) = self.chat_list_state.selected() {
        if let Some(chat) = self.chats.get(idx) {
            self.chat_menu_state = Some(ChatMenuState::new(
                chat.id.clone(),
                chat.display_name.clone().unwrap_or_else(|| chat.name.clone()),
                chat.is_pinned,
            ));
            self.input_mode = InputMode::ChatMenu;
        }
    }
}

pub fn close_chat_menu(&mut self) {
    self.chat_menu_state = None;
    self.input_mode = InputMode::Normal;
}
```

**Step 5: Build clean**

```bash
cargo build 2>&1 | grep "^error"
```

**Step 6: Commit**

```bash
git add src/tui/app_state.rs
git commit -m "feat: add ChatMenuState and InputMode::ChatMenu to app state"
```

---

### Task 4: Keybindings — new actions and `ChatMenu` mode map

**Files:**
- Modify: `src/tui/keybindings.rs`

**Step 1: Add new `Action` variants**

In the `Action` enum, add:

```rust
OpenChatMenu,
ChatMenuNext,
ChatMenuPrev,
ChatMenuConfirm,
ChatMenuClose,
```

**Step 2: Add `x` to `map_normal_mode`**

```rust
KeyCode::Char('x') => Action::OpenChatMenu,
```

**Step 3: Add `map_chat_menu_mode` function**

```rust
fn map_chat_menu_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Action::ChatMenuNext,
        KeyCode::Char('k') | KeyCode::Up => Action::ChatMenuPrev,
        KeyCode::Enter | KeyCode::Char('p') => Action::ChatMenuConfirm,
        KeyCode::Esc | KeyCode::Char('q') => Action::ChatMenuClose,
        _ => Action::None,
    }
}
```

**Step 4: Wire `ChatMenu` mode in `map_key`**

```rust
pub fn map_key(key: KeyEvent, mode: InputMode) -> Action {
    match mode {
        InputMode::Normal => map_normal_mode(key),
        InputMode::Editing => map_editing_mode(key),
        InputMode::Settings => map_settings_mode(key),
        InputMode::Renaming => map_renaming_mode(key),
        InputMode::ChatMenu => map_chat_menu_mode(key),  // NEW
    }
}
```

**Step 5: Build clean**

```bash
cargo build 2>&1 | grep "^error"
```

**Step 6: Commit**

```bash
git add src/tui/keybindings.rs
git commit -m "feat: add ChatMenu keybindings (x to open, p/Enter confirm, Esc close)"
```

---

### Task 5: Action handlers in `app.rs`

**Files:**
- Modify: `src/app.rs`

**Step 1: Add imports at top of `src/app.rs`**

Ensure `ChatMenuState` and the new actions are in scope. Check existing imports — `use crate::tui::app_state::*` likely covers it.

**Step 2: Handle `OpenChatMenu`**

```rust
Action::OpenChatMenu => {
    self.state.open_chat_menu();
}
```

**Step 3: Handle `ChatMenuNext` / `ChatMenuPrev`**

```rust
Action::ChatMenuNext => {
    if let Some(ref mut menu) = self.state.chat_menu_state {
        menu.select_next();
    }
}
Action::ChatMenuPrev => {
    if let Some(ref mut menu) = self.state.chat_menu_state {
        menu.select_prev();
    }
}
```

**Step 4: Handle `ChatMenuClose`**

```rust
Action::ChatMenuClose => {
    self.state.close_chat_menu();
}
```

**Step 5: Handle `ChatMenuConfirm`**

This is the main action — toggles pin in DB and updates the in-memory chat list:

```rust
Action::ChatMenuConfirm => {
    if let Some(ref menu) = self.state.chat_menu_state {
        let chat_id = menu.chat_id.clone();
        let new_pinned = !menu.is_pinned;
        if let Some(ref db) = self.db {
            let _ = db.set_chat_pinned(&chat_id, new_pinned);
        }
        // Update in-memory chat list
        if let Some(chat) = self.state.chats.iter_mut().find(|c| c.id == chat_id) {
            chat.is_pinned = new_pinned;
        }
        // Re-sort: pinned chats first
        self.state.chats.sort_by(|a, b| {
            b.is_pinned.cmp(&a.is_pinned)
        });
        // Reset selection to first chat to avoid out-of-bounds after re-sort
        self.state.chat_list_state.select(Some(0));
    }
    self.state.close_chat_menu();
}
```

**Step 6: Build clean**

```bash
cargo build 2>&1 | grep "^error"
```

**Step 7: Commit**

```bash
git add src/app.rs
git commit -m "feat: handle ChatMenu actions in app — pin/unpin with DB persistence"
```

---

### Task 6: Render — `chat_menu` widget

**Files:**
- Create: `src/tui/widgets/chat_menu.rs`
- Modify: `src/tui/widgets/mod.rs`
- Modify: `src/tui/render.rs`
- Modify: `src/tui/widgets/chat_list.rs`

**Step 1: Create `src/tui/widgets/chat_menu.rs`**

```rust
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
    Frame,
};

use crate::tui::app_state::ChatMenuState;

pub fn render_chat_menu(f: &mut Frame, parent_area: Rect, state: &ChatMenuState) {
    // Calculate popup size and center it over parent_area
    let popup_width = 28u16.min(parent_area.width.saturating_sub(2));
    let popup_height = (state.items.len() as u16 + 4).min(parent_area.height.saturating_sub(2));
    let x = parent_area.x + (parent_area.width.saturating_sub(popup_width)) / 2;
    let y = parent_area.y + (parent_area.height.saturating_sub(popup_height)) / 2;
    let area = Rect::new(x, y, popup_width, popup_height);

    // Clear the background before rendering the popup
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = state
        .items
        .iter()
        .map(|item| {
            ListItem::new(Line::from(Span::raw(format!(
                " {}",
                item.label(state.is_pinned)
            ))))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(" {} ", state.chat_name))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut list_state);
}
```

**Step 2: Export from `src/tui/widgets/mod.rs`**

Add:
```rust
pub mod chat_menu;
```

**Step 3: Show `*` prefix for pinned chats in `src/tui/widgets/chat_list.rs`**

In `render_chat_list`, update the item builder to show a pin indicator:

```rust
let pin_tag = if chat.is_pinned { "* " } else { "" };
let line = Line::from(vec![
    Span::styled(pin_tag, Style::default().fg(Color::Yellow)),
    Span::styled(tag, Style::default().fg(Color::DarkGray)),
    Span::raw(" "),
    Span::styled(name, Style::default().fg(Color::White)),
    Span::styled(unread, Style::default().fg(Color::Yellow)),
]);
```

**Step 4: Call `render_chat_menu` in `src/tui/render.rs`**

Import and call after all other widgets:

```rust
use super::widgets::chat_menu::render_chat_menu;

// At the end of the render function, after all other widgets:
if state.input_mode == InputMode::ChatMenu {
    if let Some(ref menu_state) = state.chat_menu_state {
        render_chat_menu(f, chat_list_area, menu_state);
    }
}
```

Note: `chat_list_area` is whatever `Rect` is used for the chat list panel — check the layout splits in `render.rs` and use the correct variable name.

**Step 5: Build clean**

```bash
cargo build 2>&1 | grep "^error"
```

**Step 6: Commit**

```bash
git add src/tui/widgets/chat_menu.rs src/tui/widgets/mod.rs src/tui/widgets/chat_list.rs src/tui/render.rs
git commit -m "feat: render chat context menu popup and pin indicator in chat list"
```

---

### Task 7: Update status bar hint and install

**Files:**
- Modify: `src/tui/widgets/status_bar.rs`

**Step 1: Add `x` hint to Normal mode in status bar**

In `status_bar.rs`, find the Normal mode hint string and add `x:Menu`:

```rust
InputMode::Normal => "q:Quit | i:Insert | s:Settings | r:Rename | x:Menu | Tab:Switch",
```

**Step 2: Add `ChatMenu` mode hint**

```rust
InputMode::ChatMenu => "j/k:Navigate | p/Enter:Confirm | Esc:Close",
```

**Step 3: Build release and install**

```bash
cargo build --release 2>&1 | grep -E "^error|Finished"
cargo install --path .
```

**Step 4: Commit**

```bash
git add src/tui/widgets/status_bar.rs
git commit -m "feat: add ChatMenu hint to status bar"
```

---

### Task 8: Manual verification checklist

Run `zero-drift-chat` and verify:

- [ ] Press `x` on a chat — context menu popup appears centered over chat list
- [ ] `j`/`k` navigates menu items (only one item for now: Pin/Unpin)
- [ ] `p` or `Enter` on "Pin" — menu closes, chat moves to top, `*` prefix appears
- [ ] Open menu again on same chat — item now shows "Unpin"
- [ ] `p` or `Enter` on "Unpin" — chat returns to normal sort order, `*` removed
- [ ] `Esc` closes menu without action
- [ ] Restart app — pinned state persists (stored in SQLite)
- [ ] Status bar shows correct hints in Normal and ChatMenu modes
