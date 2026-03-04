# Terminal Tab Title Unread Indicator Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Show `● zero-drift-chat` in the terminal tab title when any chat has unread messages, and `zero-drift-chat` when all are read.

**Architecture:** A single private helper `fn update_title(has_unread: bool)` in `src/app.rs` emits a crossterm `SetTitle` command to stdout. It is called at startup, whenever an incoming message increments an unread counter, whenever a chat is selected (clearing its unread), and on clean exit. No new state field is required.

**Tech Stack:** Rust, crossterm 0.28 (`SetTitle` in `crossterm::terminal`)

---

### Task 1: Add `update_title` helper and wire all call sites

**Files:**
- Modify: `src/app.rs`

This is the only change needed — one helper, four call sites, one import tweak.

---

**Step 1: Add `SetTitle` to the existing `crossterm::terminal` import**

In `src/app.rs`, line 7, the current import is:

```rust
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
```

Change it to:

```rust
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle},
```

---

**Step 2: Add the `update_title` helper at the bottom of the `impl App` block**

Add this just before the closing `}` of `impl App` (currently around line 554, after `clear_selected_unread`):

```rust
    fn update_title(has_unread: bool) {
        let title = if has_unread { "● zero-drift-chat" } else { "zero-drift-chat" };
        let _ = execute!(io::stdout(), SetTitle(title));
    }
```

Note: `execute!` and `io::stdout()` are already imported/used throughout this file — no additional imports needed beyond `SetTitle`.

---

**Step 3: Call `update_title` on startup**

In `run()`, after line 105 (`self.load_selected_chat_messages();`) and before `enable_raw_mode()` (line 108):

```rust
        // Set initial terminal title
        let has_unread = self.state.chats.iter().any(|c| c.unread_count > 0);
        Self::update_title(has_unread);
```

---

**Step 4: Call `update_title` when a new incoming message arrives**

In `handle_tick()`, after line 201 (the `update_unread_count` DB call), add:

```rust
                            Self::update_title(true);
```

The block context around line 198–202 is:
```rust
                        if let Some(chat) = self
                            .state
                            .chats
                            .iter_mut()
                            .find(|c| c.id == msg.chat_id)
                        {
                            chat.unread_count += 1;
                            let _ = self
                                .db
                                .update_unread_count(&chat.id, chat.unread_count);
                            // ADD HERE:
                            Self::update_title(true);
                        }
```

---

**Step 5: Call `update_title` after clearing unread on chat select**

`clear_selected_unread()` is called on lines 320 and 326 (after `NextChat` and `PrevChat`). Add a title update call after each pair:

Lines 317–321 — after `self.clear_selected_unread();`:
```rust
            Action::NextChat => {
                self.state.select_next_chat();
                self.load_selected_chat_messages();
                self.clear_selected_unread();
                self.send_read_receipts().await;
                let has_unread = self.state.chats.iter().any(|c| c.unread_count > 0);
                Self::update_title(has_unread);
            }
            Action::PrevChat => {
                self.state.select_prev_chat();
                self.load_selected_chat_messages();
                self.clear_selected_unread();
                self.send_read_receipts().await;
                let has_unread = self.state.chats.iter().any(|c| c.unread_count > 0);
                Self::update_title(has_unread);
            }
```

---

**Step 6: Reset title on clean exit**

In `run()`, in the cleanup block (around lines 154–162), before `terminal.show_cursor()`:

```rust
        // Cleanup
        self.router.stop_all().await?;
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        Self::update_title(false);   // ADD THIS LINE
        terminal.show_cursor()?;
```

---

**Step 7: Build clean**

```bash
cargo build --release 2>&1 | grep -E "^error|Finished"
```

Expected: `Finished \`release\` profile`

Fix any compile errors before continuing.

---

**Step 8: Install and manually verify**

```bash
cargo install --path .
zero-drift-chat
```

Verification checklist:
- [ ] Windows Terminal tab shows `zero-drift-chat` on startup (no unread)
- [ ] When a mock message arrives for a non-active chat, tab shows `● zero-drift-chat`
- [ ] Navigating to that chat (j/k) clears the dot → tab shows `zero-drift-chat`
- [ ] On quit (`q`), tab title returns to `zero-drift-chat`

---

**Step 9: Commit**

```bash
git add src/app.rs
git commit -m "feat: show dot in terminal tab title when there are unread messages"
```
