# Bracketed Paste Fix Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix multi-line paste sending each line as a separate message by enabling bracketed paste mode.

**Architecture:** Enable `EnableBracketedPaste` in terminal setup so the terminal wraps pasted content in escape sequences. Crossterm exposes this as `Event::Paste(String)`. Capture it in the event loop and insert the text directly into the `TextArea`, bypassing the Enter handler.

**Tech Stack:** Rust, crossterm, tui-textarea 0.7, tokio

---

## File Map

| File | Change |
|------|--------|
| `src/app.rs` | Add `EnableBracketedPaste`/`DisableBracketedPaste` to terminal lifecycle; handle `AppEvent::Paste` in main loop |
| `src/tui/event.rs` | Add `Paste(String)` to `AppEvent`; capture `Event::Paste` in event loop |

---

## Chunk 1: Terminal lifecycle + event capture

### Task 1: Add `Paste(String)` to `AppEvent` and capture in the event loop

**Files:**
- Modify: `src/tui/event.rs:7-16` (AppEvent enum)
- Modify: `src/tui/event.rs:48-64` (event match arms)

- [ ] **Step 1: Add the `Paste(String)` variant to `AppEvent`**

In `src/tui/event.rs`, change the `AppEvent` enum from:

```rust
#[derive(Debug)]
pub enum AppEvent {
    Key(crossterm::event::KeyEvent),
    Resize(u16, u16),
    Tick,
    Render,
    Quit,
    AiSuggestion(String),
    AiError(String),
}
```

to:

```rust
#[derive(Debug)]
pub enum AppEvent {
    Key(crossterm::event::KeyEvent),
    Paste(String),
    Resize(u16, u16),
    Tick,
    Render,
    Quit,
    AiSuggestion(String),
    AiError(String),
}
```

- [ ] **Step 2: Capture `Event::Paste` in the event loop**

In `src/tui/event.rs`, change the `match event` block from:

```rust
                    Some(Ok(event)) = reader.next() => {
                        match event {
                            Event::Key(key) => {
                                // Windows fix: only handle Press events to avoid duplicates
                                if key.kind == KeyEventKind::Press {
                                    if task_tx.send(AppEvent::Key(key)).is_err() {
                                        break;
                                    }
                                }
                            }
                            Event::Resize(w, h) => {
                                if task_tx.send(AppEvent::Resize(w, h)).is_err() {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
```

to:

```rust
                    Some(Ok(event)) = reader.next() => {
                        match event {
                            Event::Key(key) => {
                                // Windows fix: only handle Press events to avoid duplicates
                                if key.kind == KeyEventKind::Press {
                                    if task_tx.send(AppEvent::Key(key)).is_err() {
                                        break;
                                    }
                                }
                            }
                            Event::Paste(text) => {
                                if task_tx.send(AppEvent::Paste(text)).is_err() {
                                    break;
                                }
                            }
                            Event::Resize(w, h) => {
                                if task_tx.send(AppEvent::Resize(w, h)).is_err() {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
```

- [ ] **Step 3: Verify it compiles**

```bash
cd /home/weibin/repo/ai/zero-drift-chat
cargo check 2>&1
```

Expected: compiler errors for unhandled `AppEvent::Paste` in `app.rs` match — that's correct, it means the new variant is wired up and the next task will resolve it.

---

### Task 2: Enable bracketed paste in terminal setup and handle `AppEvent::Paste`

**Files:**
- Modify: `src/app.rs:7` (imports)
- Modify: `src/app.rs:174` (startup — enable bracketed paste)
- Modify: `src/app.rs:182` (panic hook — disable bracketed paste)
- Modify: `src/app.rs:272-275` (cleanup — disable bracketed paste)
- Modify: `src/app.rs:246-249` (main loop — handle Paste event)

- [ ] **Step 4: Add `EnableBracketedPaste` / `DisableBracketedPaste` to imports**

In `src/app.rs`, change the `use crossterm` block (lines 5-8) from:

```rust
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle},
};
```

to:

```rust
use crossterm::{
    execute,
    event::{DisableBracketedPaste, EnableBracketedPaste},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle},
};
```

- [ ] **Step 5: Enable bracketed paste on startup**

In `src/app.rs`, change:

```rust
        execute!(stdout, EnterAlternateScreen)?;
```

to:

```rust
        execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
```

- [ ] **Step 6: Disable bracketed paste in the panic hook**

In `src/app.rs`, change:

```rust
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
```

to:

```rust
            let _ = execute!(io::stdout(), DisableBracketedPaste, LeaveAlternateScreen);
```

- [ ] **Step 7: Disable bracketed paste in normal cleanup**

In `src/app.rs`, change:

```rust
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen
        )?;
```

to:

```rust
        execute!(
            terminal.backend_mut(),
            DisableBracketedPaste,
            LeaveAlternateScreen
        )?;
```

- [ ] **Step 8: Handle `AppEvent::Paste` in the main loop**

In `src/app.rs`, add the paste handler arm after the `AppEvent::Resize` arm (after line 249):

```rust
                Some(AppEvent::Paste(text)) => {
                    if self.state.input_mode == InputMode::Editing {
                        self.state.input.insert_str(&text);
                        self.state.ai_suggestion = None;
                        if self.ai_worker.is_some() {
                            self.last_keystroke = Some(Instant::now());
                        }
                    }
                }
```

- [ ] **Step 9: Verify full build passes**

```bash
cd /home/weibin/repo/ai/zero-drift-chat
cargo build 2>&1
```

Expected: `Finished` with no errors or warnings about unhandled enum variants.

- [ ] **Step 10: Manual smoke test**

```bash
cargo run
```

1. Press `i` to enter editing mode.
2. Copy a multi-line block to clipboard (e.g. three lines of text).
3. Paste with your terminal's paste shortcut (`Ctrl+Shift+V` or right-click → Paste).

Expected:
- All lines appear in the input box as one block — **no messages sent during paste**.
- Press Enter (or Ctrl+S) → the full block is sent as a single message.
- Typing Enter manually still submits immediately (behaviour unchanged).

- [ ] **Step 11: Commit**

```bash
git add src/app.rs src/tui/event.rs
git commit -m "fix: enable bracketed paste to prevent multi-line paste splitting messages"
```
