# Clickable Links in Message View Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** URLs in chat messages are visually highlighted (blue + underline), drag-to-select copies text to clipboard, and Ctrl+Click on a URL opens it in the browser.

**Architecture:** Two independent changes: (1) remove the unused `EnableMouseCapture` boilerplate to restore Windows Terminal's native drag-to-select and Ctrl+Click URL detection; (2) detect URLs in message content with a hand-rolled scanner and render them as styled `Span`s (blue + underlined) inside the existing `Paragraph` widget. No new crate dependencies.

**Tech Stack:** Rust, ratatui 0.29, crossterm 0.28. Windows Terminal handles Ctrl+Click on `http(s)://` URLs natively — no OSC 8 escape sequences required.

---

### Task 1: Remove unused EnableMouseCapture

**Files:**
- Modify: `src/app.rs`

`EnableMouseCapture` was added as Phase 1 scaffolding boilerplate and was never used — mouse events have always been silently discarded. Its only effect is disabling Windows Terminal's native text selection. Removing it restores drag-to-select (auto-copy) and Ctrl+Click URL opening.

---

**Step 1: Remove `EnableMouseCapture` and `DisableMouseCapture` from the import**

In `src/app.rs`, line 5, the current import is:

```rust
    event::{DisableMouseCapture, EnableMouseCapture},
```

Change it to:

```rust
    event::DisableMouseCapture,
```

Wait — after removing `EnableMouseCapture` entirely from all 3 call sites, we won't need `DisableMouseCapture` either. Remove the entire `event` import line:

```rust
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle},
};
```

(Keep `SetTitle` which was added for the terminal title feature.)

---

**Step 2: Remove `EnableMouseCapture` from the startup execute! call**

Current (line ~113):
```rust
execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
```

Change to:
```rust
execute!(stdout, EnterAlternateScreen)?;
```

---

**Step 3: Remove `DisableMouseCapture` from the panic hook**

Current (line ~121):
```rust
let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
```

Change to:
```rust
let _ = execute!(io::stdout(), LeaveAlternateScreen);
```

---

**Step 4: Remove `DisableMouseCapture` from the cleanup block**

Current (lines ~157-161):
```rust
execute!(
    terminal.backend_mut(),
    LeaveAlternateScreen,
    DisableMouseCapture
)?;
```

Change to:
```rust
execute!(
    terminal.backend_mut(),
    LeaveAlternateScreen
)?;
```

---

**Step 5: Build clean**

```bash
cargo build --release 2>&1 | grep -E "^error|Finished"
```

Expected: `Finished \`release\` profile`

Fix any unused import warnings that become errors.

---

**Step 6: Commit**

```bash
git add src/app.rs
git commit -m "fix: remove unused EnableMouseCapture to restore native terminal text selection"
```

---

### Task 2: URL detection and visual highlighting in message_view

**Files:**
- Modify: `src/tui/widgets/message_view.rs`

URLs are detected with a hand-rolled scanner (no `regex` crate) and rendered as blue + underlined `Span`s. Non-URL text uses the existing `msg_color`. The `Paragraph` widget and word-wrap logic are unchanged.

---

**Step 1: Add `Modifier` to the ratatui imports**

At the top of `src/tui/widgets/message_view.rs`, the current import is:

```rust
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
```

Add `Modifier` to the style imports:

```rust
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
```

---

**Step 2: Add the `split_line_with_urls` helper function**

Add this private function before `render_message_view`:

```rust
/// Split a line of text into alternating (segment, is_url) pairs.
/// URLs are defined as contiguous non-whitespace text starting with "http://" or "https://".
fn split_line_with_urls(line: &str) -> Vec<(&str, bool)> {
    let mut result = Vec::new();
    let mut remaining = line;
    while !remaining.is_empty() {
        let http_pos = remaining.find("http://");
        let https_pos = remaining.find("https://");
        let url_start = match (http_pos, https_pos) {
            (None, None) => {
                result.push((remaining, false));
                break;
            }
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (Some(a), Some(b)) => a.min(b),
        };
        if url_start > 0 {
            result.push((&remaining[..url_start], false));
        }
        let url_text = &remaining[url_start..];
        let url_end = url_text
            .find(|c: char| c.is_whitespace())
            .unwrap_or(url_text.len());
        result.push((&url_text[..url_end], true));
        remaining = &url_text[url_end..];
    }
    result
}
```

---

**Step 3: Replace the plain-text message line rendering with URL-aware rendering**

In `render_message_view`, the current message content rendering loop (lines ~59–64) is:

```rust
        for text_line in msg.content.as_text().split('\n') {
            lines.push(Line::from(Span::styled(
                text_line.to_string(),
                Style::default().fg(msg_color),
            )));
        }
```

Replace it with:

```rust
        let url_style = Style::default()
            .fg(Color::LightBlue)
            .add_modifier(Modifier::UNDERLINED);

        for text_line in msg.content.as_text().split('\n') {
            let segments = split_line_with_urls(text_line);
            if segments.iter().all(|(_, is_url)| !is_url) {
                // Fast path: no URLs — keep existing single-span behaviour
                lines.push(Line::from(Span::styled(
                    text_line.to_string(),
                    Style::default().fg(msg_color),
                )));
            } else {
                let spans: Vec<Span> = segments
                    .into_iter()
                    .map(|(seg, is_url)| {
                        if is_url {
                            Span::styled(seg.to_string(), url_style)
                        } else {
                            Span::styled(seg.to_string(), Style::default().fg(msg_color))
                        }
                    })
                    .collect();
                lines.push(Line::from(spans));
            }
        }
```

---

**Step 4: Build clean**

```bash
cargo build --release 2>&1 | grep -E "^error|Finished"
```

Expected: `Finished \`release\` profile`

---

**Step 5: Install and manually verify**

```bash
cargo install --path .
zero-drift-chat
```

Verification checklist (use the mock provider — it generates messages):
- [ ] Plain text messages render as before (no visual change)
- [ ] A message containing a URL (e.g. `https://example.com`) shows it in blue + underlined
- [ ] A message with mixed text+URL (e.g. `Check https://example.com for details`) renders correctly — plain text in normal color, URL in blue+underlined
- [ ] Drag-selecting text in the message view works and copies to clipboard (Windows Terminal)
- [ ] Ctrl+Click on a URL in the message view opens the browser (Windows Terminal)

To test URL rendering with the mock provider, you can temporarily add a URL to a mock message or trigger a message containing a URL.

---

**Step 6: Commit**

```bash
git add src/tui/widgets/message_view.rs
git commit -m "feat: highlight URLs in message view with blue underline style"
```
