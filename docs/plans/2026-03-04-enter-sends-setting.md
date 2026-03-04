# Enter-Sends Keybinding Setting Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a user-configurable `Enter to Send` toggle (default: on) stored in SQLite that takes effect immediately without restart.

**Architecture:** New `preferences` key-value table in SQLite for UI-only settings. `AppState.enter_sends: bool` loaded at startup and updated live. `map_editing_mode` receives `enter_sends` as a parameter and routes `Enter` accordingly. Settings overlay gains a new `EnterSends` Bool item saved to SQLite on Ctrl+S.

**Tech Stack:** Rust, rusqlite, ratatui 0.29, tui-textarea 0.7

**Design doc:** `docs/plans/2026-03-04-chat-pin-unpin-design.md` (for patterns reference)

---

### Task 1: SQLite `preferences` table and storage module

**Files:**
- Modify: `src/storage/db.rs`
- Create: `src/storage/preferences.rs`
- Modify: `src/storage/mod.rs`

**Step 1: Add `preferences` table migration in `src/storage/db.rs`**

In the `migrate()` function, after the existing `sessions` migration block (around line 58), add:

```rust
self.conn.execute_batch(
    "CREATE TABLE IF NOT EXISTS preferences (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );"
)?;
```

**Step 2: Create `src/storage/preferences.rs`**

```rust
use crate::core::Result;
use crate::storage::db::Database;

impl Database {
    pub fn get_preference(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT value FROM preferences WHERE key = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn set_preference(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO preferences (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }
}
```

**Step 3: Export from `src/storage/mod.rs`**

Add `mod preferences;` (private — methods are on `Database` directly via `impl`):

```rust
pub mod db;
mod addressbook;
mod chats;
mod messages;
mod preferences;
mod sessions;

pub use addressbook::AddressBook;
pub use db::Database;
```

**Step 4: Build clean**

```bash
cargo build 2>&1 | grep -E "^error|Finished"
```

**Step 5: Commit**

```bash
git add src/storage/db.rs src/storage/preferences.rs src/storage/mod.rs
git commit -m "feat: add preferences key-value table and get/set_preference methods"
```

---

### Task 2: `AppState.enter_sends` and `SettingsKey::EnterSends`

**Files:**
- Modify: `src/tui/app_state.rs`

**Step 1: Add `EnterSends` to `SettingsKey` enum**

```rust
pub enum SettingsKey {
    MockEnabled,
    WhatsAppEnabled,
    LogLevel,
    EnterSends,   // NEW
}
```

**Step 2: Add `enter_sends: bool` field to `AppState` struct**

```rust
pub struct AppState {
    // ... existing fields ...
    pub enter_sends: bool,   // NEW — loaded from DB at startup, default true
}
```

In `AppState::new()`, initialise:

```rust
enter_sends: true,   // default; overridden by DB value after construction
```

**Step 3: Add `EnterSends` item to `SettingsState::from_config`**

At the end of the `items: vec![...]` initialiser, add:

```rust
SettingsItem {
    key: SettingsKey::EnterSends,
    label: "Enter to Send".to_string(),
    value: SettingsValue::Bool(true),  // caller overwrites this after construction
},
```

**Step 4: Add `enter_sends` parameter to `SettingsState::from_config` to set initial value**

Change signature to:
```rust
pub fn from_config(config: &AppConfig, enter_sends: bool) -> Self {
```

Then set the `EnterSends` item's value from the parameter:
```rust
SettingsItem {
    key: SettingsKey::EnterSends,
    label: "Enter to Send".to_string(),
    value: SettingsValue::Bool(enter_sends),
},
```

**Step 5: Update `apply_to_config` to skip `EnterSends`**

`EnterSends` is NOT written to TOML — it's handled separately via SQLite. Add a match arm that does nothing:

```rust
(SettingsKey::EnterSends, _) => {
    // stored in SQLite preferences, not TOML config
}
```

**Step 6: Build clean**

```bash
cargo build 2>&1 | grep -E "^error|Finished"
```

Fix any compile errors from the changed `from_config` signature (call sites pass `enter_sends: bool` now).

**Step 7: Commit**

```bash
git add src/tui/app_state.rs
git commit -m "feat: add EnterSends setting key and enter_sends field to AppState"
```

---

### Task 3: Load `enter_sends` from DB at startup and wire `open_settings`

**Files:**
- Modify: `src/app.rs`

**Step 1: Read `enter_sends` from DB after `AppState::new()` in `App::new()`**

Currently `App::new` constructs `state: AppState::new()`. Add a method or do it inline in `run()` after the DB is available. The cleanest place is right after `AppState::new()` — but `App::new` doesn't call the DB yet. Instead, add an init step at the start of `App::run()`:

```rust
// Load enter_sends preference from DB (default true)
self.state.enter_sends = self.db
    .get_preference("enter_sends")
    .ok()
    .flatten()
    .map(|v| v != "false")
    .unwrap_or(true);
```

Place this before the provider registration block in `run()`.

**Step 2: Fix `open_settings` call to pass `enter_sends`**

Find where `Action::OpenSettings` is handled in `app.rs`. It calls `self.state.open_settings(&self.config)`. Update `open_settings` signature in `app_state.rs`:

```rust
pub fn open_settings(&mut self, config: &AppConfig, enter_sends: bool) {
    self.settings_state = Some(SettingsState::from_config(config, enter_sends));
    self.input_mode = InputMode::Settings;
}
```

Update the call site in `app.rs`:

```rust
Action::OpenSettings => {
    self.state.open_settings(&self.config, self.state.enter_sends);
}
```

**Step 3: Handle `EnterSends` in `SettingsSave`**

In the `Action::SettingsSave` handler, after `settings.apply_to_config(&mut self.config)` and the TOML save, add:

```rust
// Save EnterSends to SQLite and apply live
if let Some(ref settings) = self.state.settings_state {
    if let Some(item) = settings.items.iter().find(|i| i.key == SettingsKey::EnterSends) {
        if let SettingsValue::Bool(v) = item.value {
            let _ = self.db.set_preference("enter_sends", if v { "true" } else { "false" });
            self.state.enter_sends = v;
        }
    }
}
```

Note: this block must come BEFORE `self.state.close_settings()` since it reads `settings_state`.

**Step 4: Build clean**

```bash
cargo build 2>&1 | grep -E "^error|Finished"
```

**Step 5: Commit**

```bash
git add src/app.rs src/tui/app_state.rs
git commit -m "feat: load enter_sends from SQLite at startup, save live on settings Ctrl+S"
```

---

### Task 4: Update `map_key` / `map_editing_mode` to use `enter_sends`

**Files:**
- Modify: `src/tui/keybindings.rs`
- Modify: `src/app.rs` (call site)

**Step 1: Update `map_key` signature to accept `enter_sends`**

```rust
pub fn map_key(key: KeyEvent, mode: InputMode, enter_sends: bool) -> Action {
    match mode {
        InputMode::Normal   => map_normal_mode(key),
        InputMode::Editing  => map_editing_mode(key, enter_sends),
        InputMode::Settings => map_settings_mode(key),
        InputMode::Renaming => map_renaming_mode(key),
        InputMode::ChatMenu => map_chat_menu_mode(key),
    }
}
```

**Step 2: Update `map_editing_mode` to branch on `enter_sends`**

```rust
fn map_editing_mode(key: KeyEvent, enter_sends: bool) -> Action {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => Action::ExitEditing,

        // When enter_sends=true: Enter submits, Shift+Enter/Alt+Enter insert newline
        (KeyCode::Enter, m)
            if enter_sends && m == KeyModifiers::NONE => Action::SubmitMessage,
        (KeyCode::Enter, m)
            if enter_sends && (m.contains(KeyModifiers::SHIFT) || m.contains(KeyModifiers::ALT))
            => Action::InputKey(key),  // forward to textarea as newline

        // When enter_sends=false (original): Shift+Enter/Alt+Enter submit, Enter inserts newline
        (KeyCode::Enter, m)
            if !enter_sends && m.contains(KeyModifiers::SHIFT) => Action::SubmitMessage,
        (KeyCode::Enter, m)
            if !enter_sends && m.contains(KeyModifiers::ALT)   => Action::SubmitMessage,

        // Ctrl+S always submits regardless of mode
        (KeyCode::Char('s'), m) if m.contains(KeyModifiers::CONTROL) => Action::SubmitMessage,
        // Ctrl+U always clears
        (KeyCode::Char('u'), m) if m.contains(KeyModifiers::CONTROL) => Action::ClearInput,
        // Everything else forwarded to TextArea
        _ => Action::InputKey(key),
    }
}
```

**Step 3: Update the `map_key` call site in `src/app.rs`**

Find line ~131:
```rust
let action = map_key(key, self.state.input_mode);
```
Change to:
```rust
let action = map_key(key, self.state.input_mode, self.state.enter_sends);
```

**Step 4: Build clean**

```bash
cargo build 2>&1 | grep -E "^error|Finished"
```

**Step 5: Commit**

```bash
git add src/tui/keybindings.rs src/app.rs
git commit -m "feat: route Enter key based on enter_sends setting in map_editing_mode"
```

---

### Task 5: Update status bar hint to reflect active mode

**Files:**
- Modify: `src/tui/widgets/status_bar.rs`
- Modify: `src/tui/render.rs` (to pass `enter_sends`)

**Step 1: Read `src/tui/widgets/status_bar.rs` and `src/tui/render.rs` first**

Understand the current `render_status_bar` signature and how it's called from `render.rs`.

**Step 2: Add `enter_sends: bool` parameter to `render_status_bar`**

Update the function signature:
```rust
pub fn render_status_bar(f: &mut Frame, area: Rect, mode: InputMode, enter_sends: bool) {
```

**Step 3: Update the `Editing` mode hint to reflect active mode**

Replace the current hardcoded Editing hint with a dynamic one:

```rust
InputMode::Editing => {
    if enter_sends {
        "Esc:Normal | Enter:Send | Shift+Enter:Newline | Ctrl+S:Send | Ctrl+U:Clear"
    } else {
        "Esc:Normal | Enter:Newline | Shift+Enter/Ctrl+S:Send | Ctrl+U:Clear"
    }
}
```

**Step 4: Update call site in `src/tui/render.rs`**

Find the `render_status_bar(...)` call and add `state.enter_sends`:
```rust
widgets::status_bar::render_status_bar(f, status_area, state.input_mode, state.enter_sends);
```

**Step 5: Build clean**

```bash
cargo build 2>&1 | grep -E "^error|Finished"
```

**Step 6: Build release and install**

```bash
cargo build --release 2>&1 | grep -E "^error|Finished"
cargo install --path .
```

**Step 7: Commit**

```bash
git add src/tui/widgets/status_bar.rs src/tui/render.rs
git commit -m "feat: dynamic status bar hint reflects enter_sends mode"
```

---

### Task 6: Manual verification checklist

Run `zero-drift-chat` and verify:

- [ ] Default mode: pressing `Enter` in edit mode sends the message immediately
- [ ] Default mode: `Shift+Enter` or `Ctrl+S` inserts a newline (forwards to textarea)
- [ ] Status bar shows `Enter:Send | Shift+Enter:Newline | Ctrl+S:Send` by default
- [ ] Open Settings (`s`) → `Enter to Send` item appears with `[x] On`
- [ ] Toggle `Enter to Send` off (Enter/Space) → shows `[ ] Off`
- [ ] Ctrl+S saves → setting takes effect immediately (no restart)
- [ ] In `enter_sends=false` mode: `Enter` inserts newline, `Ctrl+S` sends
- [ ] Status bar updates to `Enter:Newline | Shift+Enter/Ctrl+S:Send`
- [ ] Restart app → setting persists from SQLite
- [ ] Rename mode (`r`) unaffected — plain Enter still confirms rename
