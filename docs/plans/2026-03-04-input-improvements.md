# Input Improvements Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the custom input box with `tui-textarea` to fix the text-hidden-when-long bug (#5) and add cursor navigation, multi-line input, and cross-platform send keybindings (#2).

**Architecture:** Four sequential tasks touching five files. `tui-textarea` handles all cursor/scroll/Unicode logic; we intercept only three special keys (Esc, send, clear) before forwarding the rest. The Renaming mode is migrated alongside Editing to use the same TextArea field.

**Tech Stack:** Rust, ratatui 0.29, tui-textarea 0.7, crossterm 0.28

---

### Task 1: Add tui-textarea dependency

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add the dependency**

Open `Cargo.toml`. The `[dependencies]` section currently ends around line 26 with `qrcode = "0.14"`. Add one line after it:

```toml
tui-textarea = { version = "0.7", features = ["crossterm"] }
```

The `crossterm` feature enables `impl From<crossterm::event::KeyEvent> for tui_textarea::Input`, which is needed to forward key events directly.

**Step 2: Verify it resolves**

```bash
cd /home/weibin/repo/ai/zero-drift-chat
cargo check 2>&1 | tail -5
```

Expected: ends with `warning: ...` lines or `Finished checking`. No `error` lines.

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add tui-textarea 0.7 for input improvements"
```

---

### Task 2: Migrate AppState from input_buffer/cursor_position to TextArea

**Files:**
- Modify: `src/tui/app_state.rs`

**Context:** `AppState` (line 127) currently has `input_buffer: String` (line 133) and `cursor_position: usize` (line 134). The methods `push_char()` (line 224), `delete_char()` (line 229), and `take_input()` (line 241) manage them. `enter_editing()` (line 215) and `exit_editing()` (line 220) toggle `input_mode`.

**Step 1: Add the import**

At the top of `src/tui/app_state.rs`, add after `use ratatui::widgets::ListState;`:

```rust
use tui_textarea::TextArea;
```

**Step 2: Replace the two fields in the AppState struct**

Replace lines 133–134:
```rust
    pub input_buffer: String,
    pub cursor_position: usize,
```

With:
```rust
    pub input: TextArea<'static>,
```

**Step 3: Update AppState::new()**

Replace lines 154–155 in `new()`:
```rust
            input_buffer: String::new(),
            cursor_position: 0,
```

With:
```rust
            input: TextArea::default(),
```

**Step 4: Remove push_char() and delete_char(), update take_input()**

Remove the entire `push_char()` method (lines 224–227) and the entire `delete_char()` method (lines 229–239).

Replace the `take_input()` method (lines 241–244) with:

```rust
    pub fn take_input(&mut self) -> String {
        let text = self.input.lines().join("\n");
        self.input = TextArea::default();
        text
    }
```

**Step 5: Verify compilation**

```bash
cargo check 2>&1 | grep "^error" | head -20
```

Expected: errors referencing `input_buffer` and `cursor_position` in `src/app.rs` — these are expected at this stage because `app.rs` still references the old fields. Zero errors in `app_state.rs` itself.

**Step 6: Commit**

```bash
git add src/tui/app_state.rs
git commit -m "refactor: replace input_buffer/cursor_position with TextArea in AppState"
```

---

### Task 3: Update keybindings.rs

**Files:**
- Modify: `src/tui/keybindings.rs`

**Context:** The `Action` enum (line 6) has `DeleteChar` (line 14) and `InsertChar(char)` (line 15). `map_editing_mode()` (line 55) maps `Enter` to `SubmitMessage` and chars to `InsertChar`. `map_renaming_mode()` (line 76) also uses `DeleteChar` and `InsertChar`.

**Step 1: Update the Action enum**

Replace the entire `Action` enum (lines 5–28) with:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    SwitchPanel,
    NextChat,
    PrevChat,
    EnterEditing,
    ExitEditing,
    SubmitMessage,
    ClearInput,
    InputKey(KeyEvent),
    ScrollUp,
    ScrollDown,
    OpenSettings,
    SettingsNext,
    SettingsPrev,
    SettingsToggle,
    SettingsSave,
    SettingsClose,
    RenameChat,
    ConfirmRename,
    CancelRename,
    None,
}
```

`DeleteChar` and `InsertChar(char)` are removed. `ClearInput` and `InputKey(KeyEvent)` are added.

**Step 2: Replace map_editing_mode()**

Replace the entire `map_editing_mode()` function (lines 55–63) with:

```rust
fn map_editing_mode(key: KeyEvent) -> Action {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => Action::ExitEditing,
        // Shift+Enter: works on Windows Terminal, iTerm2, modern macOS terminals, WSL
        (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => Action::SubmitMessage,
        // Alt+Enter: fallback for macOS Terminal.app and other terminals
        (KeyCode::Enter, m) if m.contains(KeyModifiers::ALT) => Action::SubmitMessage,
        // Ctrl+U: clear entire buffer (override tui-textarea default of delete-to-line-start)
        (KeyCode::Char('u'), m) if m.contains(KeyModifiers::CONTROL) => Action::ClearInput,
        // All other keys forwarded to TextArea (handles arrows, backspace, enter=newline, home/end, etc.)
        _ => Action::InputKey(key),
    }
}
```

**Step 3: Replace map_renaming_mode()**

Replace the entire `map_renaming_mode()` function (lines 76–84) with:

```rust
fn map_renaming_mode(key: KeyEvent) -> Action {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => Action::CancelRename,
        // Plain Enter confirms rename (single-line context, no multi-line needed)
        (KeyCode::Enter, m) if m == KeyModifiers::NONE => Action::ConfirmRename,
        // All other keys forwarded to TextArea (handles arrows, backspace, home/end)
        _ => Action::InputKey(key),
    }
}
```

**Step 4: Verify compilation**

```bash
cargo check 2>&1 | grep "^error" | head -20
```

Expected: errors in `src/app.rs` only (still references `DeleteChar`, `InsertChar`, `input_buffer`, `cursor_position`). Zero errors in `keybindings.rs`.

**Step 5: Commit**

```bash
git add src/tui/keybindings.rs
git commit -m "refactor: replace InsertChar/DeleteChar actions with InputKey/ClearInput"
```

---

### Task 4: Update app.rs action handler

**Files:**
- Modify: `src/app.rs`

**Context:** `handle_action()` (line 297) handles `DeleteChar` (line 352), `InsertChar` (line 355), `RenameChat` (line 396), `CancelRename` (line 422). `RenameChat` directly writes to `self.state.input_buffer` (line 404) and `self.state.cursor_position` (line 405). `CancelRename` clears them (lines 423–425).

**Step 1: Add the tui_textarea import**

At the top of `src/app.rs`, add to the existing use statements:

```rust
use tui_textarea::TextArea;
```

**Step 2: Replace DeleteChar and InsertChar handlers**

Remove lines 352–357:
```rust
            Action::DeleteChar => {
                self.state.delete_char();
            }
            Action::InsertChar(c) => {
                self.state.push_char(c);
            }
```

Replace with:
```rust
            Action::InputKey(key) => {
                self.state.input.input(key);
            }
            Action::ClearInput => {
                self.state.input = TextArea::default();
            }
```

**Step 3: Replace RenameChat handler**

Replace lines 396–409:
```rust
            Action::RenameChat => {
                if let Some(idx) = self.state.chat_list_state.selected() {
                    if let Some(chat) = self.state.chats.get(idx) {
                        let name = chat
                            .display_name
                            .as_ref()
                            .unwrap_or(&chat.name)
                            .clone();
                        self.state.input_buffer = name;
                        self.state.cursor_position = self.state.input_buffer.len();
                        self.state.input_mode = InputMode::Renaming;
                    }
                }
            }
```

With:
```rust
            Action::RenameChat => {
                if let Some(idx) = self.state.chat_list_state.selected() {
                    if let Some(chat) = self.state.chats.get(idx) {
                        let name = chat
                            .display_name
                            .as_ref()
                            .unwrap_or(&chat.name)
                            .clone();
                        let mut ta = TextArea::from(vec![name]);
                        ta.move_cursor(tui_textarea::CursorMove::End);
                        self.state.input = ta;
                        self.state.input_mode = InputMode::Renaming;
                    }
                }
            }
```

**Step 4: Replace CancelRename handler**

Replace lines 422–426:
```rust
            Action::CancelRename => {
                self.state.input_buffer.clear();
                self.state.cursor_position = 0;
                self.state.input_mode = InputMode::Normal;
            }
```

With:
```rust
            Action::CancelRename => {
                self.state.input = TextArea::default();
                self.state.input_mode = InputMode::Normal;
            }
```

**Step 5: Verify compilation**

```bash
cargo check 2>&1 | grep "^error" | head -20
```

Expected: errors only in `src/tui/render.rs` and `src/tui/widgets/input_bar.rs` (still pass old `input_buffer`/`cursor_position` to `render_input_bar`). Zero errors in `app.rs`.

**Step 6: Commit**

```bash
git add src/app.rs
git commit -m "refactor: update app.rs to use TextArea for input handling"
```

---

### Task 5: Update render.rs and input_bar.rs

**Files:**
- Modify: `src/tui/render.rs`
- Modify: `src/tui/widgets/input_bar.rs`

**Context:** `render.rs` calls `render_input_bar(f, input_area, &state.input_buffer, state.cursor_position, state.input_mode)` (lines 59–65). `input_bar.rs` renders a `Paragraph` with a mode-tag prefix and positions the hardware cursor manually (lines 11–50).

**Step 1: Rewrite input_bar.rs**

Replace the entire content of `src/tui/widgets/input_bar.rs` with:

```rust
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use tui_textarea::TextArea;

use crate::tui::app_state::InputMode;

pub fn render_input_bar(
    f: &mut Frame,
    area: Rect,
    textarea: &TextArea<'static>,
    mode: InputMode,
) {
    let (mode_tag, border_color) = match mode {
        InputMode::Normal => ("NORMAL", Color::DarkGray),
        InputMode::Editing => ("INSERT", Color::Yellow),
        InputMode::Settings => ("SETTINGS", Color::Cyan),
        InputMode::Renaming => ("RENAME", Color::Magenta),
    };

    let block = Block::default()
        .title(format!(" [{}] ", mode_tag))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if mode == InputMode::Editing || mode == InputMode::Renaming {
        // TextArea widget: handles cursor positioning, scrolling, multi-line display
        f.render_widget(textarea.widget(), inner_area);
    } else {
        // Normal/Settings: render text only, no hardware cursor in the input box
        let text = textarea.lines().join("\n");
        f.render_widget(Paragraph::new(text), inner_area);
    }
}
```

Key changes from the old implementation:
- Mode tag moves from inline text prefix to the block title (cleaner UX)
- In Editing/Renaming modes: `textarea.widget()` handles cursor display, horizontal scroll, and multi-line rendering automatically
- In Normal/Settings modes: plain `Paragraph` so the hardware cursor does not appear in the input box

**Step 2: Update the call site in render.rs**

Replace lines 59–65 in `src/tui/render.rs`:
```rust
    input_bar::render_input_bar(
        f,
        input_area,
        &state.input_buffer,
        state.cursor_position,
        state.input_mode,
    );
```

With:
```rust
    input_bar::render_input_bar(
        f,
        input_area,
        &state.input,
        state.input_mode,
    );
```

**Step 3: Full build**

```bash
cargo build 2>&1 | grep "^error" | head -20
```

Expected: no `error` lines. Warnings about unused code are fine.

**Step 4: Smoke test**

```bash
cargo run -- --mock 2>&1 &
```

Verify manually:
- In Normal mode: input box shows `[NORMAL]` title, no blinking cursor inside
- Press `i` to enter editing: title changes to `[INSERT]`, cursor appears
- Type a short message: text appears, cursor moves right
- Type a very long message (> box width): text scrolls horizontally, cursor stays visible
- Press `←`/`→`: cursor moves within text
- Press `Home`/`End`: cursor jumps to start/end of line
- Press `Enter`: newline inserted (multi-line input)
- Press `Shift+Enter` or `Alt+Enter`: message sent
- Press `Ctrl+U`: entire input cleared
- Press `Esc`: exits editing mode
- Press `r` to rename a chat: box pre-filled with current name in RENAME mode, arrows work, Enter confirms

Kill the process with `Ctrl+C` when done.

**Step 5: Commit**

```bash
git add src/tui/widgets/input_bar.rs src/tui/render.rs
git commit -m "feat: replace input Paragraph with tui-textarea (fixes #5, closes #2 part 1)"
```
