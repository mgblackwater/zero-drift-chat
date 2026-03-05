# Chat Search Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Press `/` in Normal mode to open a fuzzy-search popup over the chat list; Enter navigates to the selected chat and enters Insert mode.

**Architecture:** New `InputMode::Searching` follows the same pattern as `Renaming`/`ChatMenu` — state struct on `AppState`, action variants, keybinding map, overlay widget rendered last in `render.rs`. Fuzzy match lives in its own module (`src/tui/search.rs`) so it can be unit-tested in isolation.

**Tech Stack:** Rust, ratatui (widgets: `List`, `Block`, `Paragraph`, `Clear`), crossterm KeyEvent, existing `UnifiedChat` type.

---

### Task 1: Fuzzy match module with unit tests

**Files:**
- Create: `src/tui/search.rs`
- Modify: `src/tui/mod.rs` (add `pub mod search;`)

**Step 1: Create `src/tui/search.rs` with the functions and tests**

```rust
use crate::core::types::UnifiedChat;

/// Returns the match span length (lower = tighter match = better).
/// All chars of `query` must appear in order in `text` (case-insensitive).
/// Returns None if no match.
pub fn fuzzy_score(query: &str, text: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    let query_chars: Vec<char> = query.to_lowercase().chars().collect();
    let text_chars: Vec<char> = text.to_lowercase().chars().collect();
    let mut qi = 0;
    let mut first_match: Option<usize> = None;
    let mut last_match = 0;
    for (ti, &tc) in text_chars.iter().enumerate() {
        if tc == query_chars[qi] {
            if first_match.is_none() {
                first_match = Some(ti);
            }
            last_match = ti;
            qi += 1;
            if qi == query_chars.len() {
                return Some(last_match - first_match.unwrap());
            }
        }
    }
    None
}

/// Returns up to `limit` chat indices from `chats`, sorted by fuzzy score (best first).
/// Returns empty vec when query is empty.
pub fn top_fuzzy_matches(query: &str, chats: &[UnifiedChat], limit: usize) -> Vec<usize> {
    if query.is_empty() {
        return vec![];
    }
    let mut scored: Vec<(usize, usize)> = chats
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let name = c.display_name.as_deref().unwrap_or(&c.name);
            fuzzy_score(query, name).map(|s| (i, s))
        })
        .collect();
    scored.sort_by_key(|&(_, s)| s);
    scored.into_iter().take(limit).map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_score_exact() {
        assert_eq!(fuzzy_score("xin", "xin wei"), Some(2));
    }

    #[test]
    fn test_fuzzy_score_scattered() {
        // 'x' at 0, 'w' at 4 → span 4
        assert_eq!(fuzzy_score("xw", "xin wei"), Some(4));
    }

    #[test]
    fn test_fuzzy_score_no_match() {
        assert_eq!(fuzzy_score("zzz", "xin wei"), None);
    }

    #[test]
    fn test_fuzzy_score_empty_query() {
        assert_eq!(fuzzy_score("", "anything"), Some(0));
    }

    #[test]
    fn test_fuzzy_score_case_insensitive() {
        assert_eq!(fuzzy_score("XIN", "xin wei"), Some(2));
    }

    #[test]
    fn test_top_fuzzy_matches_empty_query() {
        let chats = vec![make_chat("alice"), make_chat("bob")];
        assert!(top_fuzzy_matches("", &chats, 5).is_empty());
    }

    #[test]
    fn test_top_fuzzy_matches_limit() {
        let chats = vec![
            make_chat("alice"),
            make_chat("alan"),
            make_chat("alex"),
            make_chat("albert"),
            make_chat("alvin"),
            make_chat("aldous"),
        ];
        let results = top_fuzzy_matches("al", &chats, 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_top_fuzzy_matches_sorted_by_score() {
        // "xin" scores 2 on "xin wei", higher span on "xi_n_long"
        let chats = vec![make_chat("xi_n_long"), make_chat("xin wei")];
        let results = top_fuzzy_matches("xin", &chats, 5);
        // "xin wei" (index 1) should rank first (tighter match)
        assert_eq!(results[0], 1);
    }

    fn make_chat(name: &str) -> UnifiedChat {
        UnifiedChat {
            id: name.to_string(),
            name: name.to_string(),
            display_name: None,
            platform: crate::core::types::Platform::Mock,
            last_message: None,
            last_message_time: None,
            unread_count: 0,
            is_pinned: false,
            is_newsletter: false,
        }
    }
}
```

**Step 2: Add `pub mod search;` to `src/tui/mod.rs`**

The file currently lists modules like `pub mod app_state;`. Add `pub mod search;` to the list.

**Step 3: Run tests**

```bash
cargo test tui::search
```

Expected: all 8 tests pass.

**Step 4: Commit**

```bash
git add src/tui/search.rs src/tui/mod.rs
git commit -m "feat: add fuzzy chat search module with unit tests"
```

---

### Task 2: State types — `InputMode::Searching` and `SearchState`

**Files:**
- Modify: `src/tui/app_state.rs`

**Step 1: Add `Searching` to `InputMode` enum**

In `app_state.rs`, the enum currently ends with `ChatMenu`. Add `Searching`:

```rust
pub enum InputMode {
    Normal,
    Editing,
    Settings,
    Renaming,
    ChatMenu,
    Searching,
}
```

**Step 2: Add `SearchState` struct above `AppState`**

```rust
pub struct SearchState {
    pub query: String,
    pub results: Vec<usize>,  // indices into AppState::chats (top 5)
    pub selected: usize,      // currently highlighted result index
}

impl SearchState {
    pub fn new() -> Self {
        Self { query: String::new(), results: vec![], selected: 0 }
    }
}
```

**Step 3: Add `search_state` field to `AppState`**

In the `AppState` struct, after `chat_menu_state`:
```rust
pub search_state: Option<SearchState>,
```

In `AppState::new()`, initialise it:
```rust
search_state: None,
```

**Step 4: Build — compiler will show every match that needs a new arm**

```bash
cargo build 2>&1 | grep "error"
```

Expected: errors on `input_bar.rs` and `status_bar.rs` match arms — note them, fix in Task 4.

**Step 5: Commit**

```bash
git add src/tui/app_state.rs
git commit -m "feat: add InputMode::Searching and SearchState"
```

---

### Task 3: Actions and keybindings

**Files:**
- Modify: `src/tui/keybindings.rs`

**Step 1: Add new `Action` variants**

In the `Action` enum, after `ChatMenuClose`:
```rust
OpenSearch,
SearchInput(KeyEvent),
SearchNext,
SearchPrev,
SearchConfirm,
SearchClose,
```

**Step 2: Wire `'/'` in `map_normal_mode`**

In `map_normal_mode`, after the `'x'` arm:
```rust
KeyCode::Char('/') => Action::OpenSearch,
```

**Step 3: Add `map_search_mode` function**

At the bottom of the file:
```rust
fn map_search_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::SearchClose,
        KeyCode::Enter => Action::SearchConfirm,
        KeyCode::Char('j') | KeyCode::Down => Action::SearchNext,
        KeyCode::Char('k') | KeyCode::Up => Action::SearchPrev,
        _ => Action::SearchInput(key),
    }
}
```

**Step 4: Add arm to `map_key`**

In `map_key`, after `InputMode::ChatMenu`:
```rust
InputMode::Searching => map_search_mode(key),
```

**Step 5: Build**

```bash
cargo build 2>&1 | grep "error"
```

Expected: only errors from `input_bar`/`status_bar` non-exhaustive matches.

**Step 6: Commit**

```bash
git add src/tui/keybindings.rs
git commit -m "feat: add search actions and keybindings"
```

---

### Task 4: Update `input_bar` and `status_bar`

**Files:**
- Modify: `src/tui/widgets/input_bar.rs`
- Modify: `src/tui/widgets/status_bar.rs`

**Step 1: Add `Searching` arm to `input_bar.rs`**

In the `match mode` block for `(mode_tag, border_color)`:
```rust
InputMode::Searching => ("SEARCH", Color::Blue),
```

**Step 2: Add `Searching` arm to `status_bar.rs`**

In the `match mode` block for `hints`:
```rust
InputMode::Searching => "Type to filter | j/k:Navigate | Enter:Open+Insert | Esc:Cancel",
```

**Step 3: Build — should be clean now**

```bash
cargo build 2>&1 | grep "error"
```

Expected: no errors.

**Step 4: Commit**

```bash
git add src/tui/widgets/input_bar.rs src/tui/widgets/status_bar.rs
git commit -m "feat: add Searching mode to input_bar and status_bar"
```

---

### Task 5: Search overlay widget

**Files:**
- Create: `src/tui/widgets/search_overlay.rs`
- Modify: `src/tui/widgets/mod.rs`
- Modify: `src/tui/render.rs`

**Step 1: Create `src/tui/widgets/search_overlay.rs`**

```rust
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::core::types::UnifiedChat;
use crate::tui::app_state::SearchState;

pub fn render_search_overlay(
    f: &mut Frame,
    chat_list_area: Rect,
    state: &SearchState,
    chats: &[UnifiedChat],
) {
    let result_count = state.results.len().min(5);
    // 2 borders + 1 query line + (1 divider + N results) if any results
    let inner_height = 1 + if result_count > 0 { 1 + result_count } else { 0 };
    let height = (2 + inner_height as u16).min(chat_list_area.height);

    let popup_area = Rect {
        x: chat_list_area.x,
        y: chat_list_area.y,
        width: chat_list_area.width,
        height,
    };

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Find Chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    // Query line
    let query_area = Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 };
    f.render_widget(
        Paragraph::new(format!("/ {}▌", state.query))
            .style(Style::default().fg(Color::White)),
        query_area,
    );

    if result_count == 0 {
        return;
    }

    // Divider
    let sep_area = Rect { x: inner.x, y: inner.y + 1, width: inner.width, height: 1 };
    f.render_widget(
        Paragraph::new("─".repeat(inner.width as usize))
            .style(Style::default().fg(Color::DarkGray)),
        sep_area,
    );

    // Results list
    let results_area = Rect {
        x: inner.x,
        y: inner.y + 2,
        width: inner.width,
        height: result_count as u16,
    };

    let highlight = Style::default()
        .bg(Color::Blue)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    let items: Vec<ListItem> = state
        .results
        .iter()
        .enumerate()
        .map(|(pos, &idx)| {
            let chat = &chats[idx];
            let name = chat.display_name.as_deref().unwrap_or(&chat.name);
            let selector = if pos == state.selected { "▶ " } else { "  " };
            let tag = format!("[{}] ", chat.platform);
            ListItem::new(Line::from(vec![
                Span::raw(selector),
                Span::styled(tag, Style::default().fg(Color::DarkGray)),
                Span::styled(name.to_string(), Style::default().fg(Color::White)),
            ]))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));
    f.render_stateful_widget(List::new(items).highlight_style(highlight), results_area, &mut list_state);
}
```

**Step 2: Add `pub mod search_overlay;` to `src/tui/widgets/mod.rs`**

Append to the existing module list.

**Step 3: Add render call to `src/tui/render.rs`**

After the ChatMenu overlay block (at the bottom of `draw`), add:
```rust
// Render search overlay on top if active
if state.input_mode == InputMode::Searching {
    if let Some(ref search) = state.search_state {
        widgets::search_overlay::render_search_overlay(f, chat_list_area, search, &state.chats);
    }
}
```

Also add `search_overlay` to the import in `render.rs`:
```rust
use super::widgets::{self, chat_list, input_bar, message_view, qr_overlay, search_overlay, settings_overlay, status_bar};
```

Actually, since we call it via `widgets::search_overlay::render_search_overlay`, the `use` import just needs `widgets` in scope which it already has. No change needed to the import line.

**Step 4: Build**

```bash
cargo build 2>&1 | grep "error"
```

Expected: no errors.

**Step 5: Commit**

```bash
git add src/tui/widgets/search_overlay.rs src/tui/widgets/mod.rs src/tui/render.rs
git commit -m "feat: add search overlay widget"
```

---

### Task 6: Handle search actions in `app.rs`

**Files:**
- Modify: `src/app.rs`

**Step 1: Add import for `top_fuzzy_matches` and `SearchState`**

At the top of `src/app.rs`, the existing import line for app_state is:
```rust
use crate::tui::app_state::{AppState, ChatMenuItem, InputMode, SettingsKey, SettingsValue};
```

Add `SearchState` to that list:
```rust
use crate::tui::app_state::{AppState, ChatMenuItem, InputMode, SearchState, SettingsKey, SettingsValue};
```

Add the search module import (near the other `crate::tui` imports):
```rust
use crate::tui::search::top_fuzzy_matches;
```

**Step 2: Add `OpenSearch` arm in `handle_action`**

Find where `Action::OpenChatMenu` is handled. Add a new arm nearby:
```rust
Action::OpenSearch => {
    self.state.search_state = Some(SearchState::new());
    self.state.input_mode = InputMode::Searching;
}
```

**Step 3: Add `SearchClose` arm**

```rust
Action::SearchClose => {
    self.state.search_state = None;
    self.state.input_mode = InputMode::Normal;
}
```

**Step 4: Add `SearchInput` arm**

```rust
Action::SearchInput(key) => {
    use crossterm::event::KeyCode;
    if let Some(ref mut ss) = self.state.search_state {
        match key.code {
            KeyCode::Backspace => { ss.query.pop(); }
            KeyCode::Char(c) => { ss.query.push(c); }
            _ => {}
        }
        ss.results = top_fuzzy_matches(&ss.query, &self.state.chats, 5);
        ss.selected = ss.selected.min(ss.results.len().saturating_sub(1));
    }
}
```

**Step 5: Add `SearchNext` and `SearchPrev` arms**

```rust
Action::SearchNext => {
    if let Some(ref mut ss) = self.state.search_state {
        if !ss.results.is_empty() {
            ss.selected = (ss.selected + 1) % ss.results.len();
        }
    }
}
Action::SearchPrev => {
    if let Some(ref mut ss) = self.state.search_state {
        if !ss.results.is_empty() {
            ss.selected = ss.selected.checked_sub(1).unwrap_or(ss.results.len() - 1);
        }
    }
}
```

**Step 6: Add `SearchConfirm` arm**

```rust
Action::SearchConfirm => {
    let chat_idx = self.state.search_state.as_ref()
        .and_then(|ss| ss.results.get(ss.selected).copied());
    if let Some(idx) = chat_idx {
        self.state.search_state = None;
        self.state.input_mode = InputMode::Editing;
        self.state.chat_list_state.select(Some(idx));
        self.load_selected_chat_messages();
        self.capture_new_message_count();
        self.clear_selected_unread();
        self.send_read_receipts().await;
        self.refresh_title();
    }
}
```

**Step 7: Full build**

```bash
cargo build 2>&1 | grep "error"
```

Expected: no errors, only existing warnings.

**Step 8: Run all tests**

```bash
cargo test
```

Expected: all tests pass including the 8 fuzzy match tests.

**Step 9: Commit**

```bash
git add src/app.rs
git commit -m "feat: handle search actions — open, input, navigate, confirm, close"
```

---

### Task 7: Install and verify

**Step 1: Install**

```bash
cargo install --path .
```

**Step 2: Manual verification**

1. Run `zero-drift-chat`
2. Press `/` → popup appears with `Find Chat` border, query line shows `/ ▌`, status bar shows search hints
3. Type part of a chat name → up to 5 fuzzy results appear below the divider
4. Press `j`/`k` → `▶` selector moves between results
5. Press `Enter` → chat list jumps to that chat, insert mode activates (input bar shows `[INSERT]`)
6. Press `/` again, then `Esc` → popup closes, back to Normal mode

**Step 3: Final commit**

```bash
git add -u
git commit -m "chore: verify chat search feature complete"
```
