# Message Layout + Group Chat Indicator Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Right-align outgoing messages in the message view, and show a `[GP]` (magenta) indicator for group chats in the chat list.

**Architecture:** Two independent rendering changes in two widget files. Task 1 adds an `else if chat.is_group` branch in `chat_list.rs`. Task 2 adds `Alignment::Right` to outgoing message `Line`s in `message_view.rs` and reverses the header order (time before sender) so right-aligned headers read naturally.

**Tech Stack:** Rust, ratatui 0.29

---

## File Map

| File | Change |
|------|--------|
| `src/tui/widgets/chat_list.rs` | Add `[GP]` in Magenta when `chat.is_group`, between the `[NL]` and platform-tag branches |
| `src/tui/widgets/message_view.rs` | Import `Alignment`; apply `.alignment(Alignment::Right)` to outgoing header + content lines; reverse header span order for outgoing |

---

## Chunk 1: Group chat indicator

### Task 1: Show `[GP]` in Magenta for group chats in the chat list

**Files:**
- Modify: `src/tui/widgets/chat_list.rs:34-38`

The current tag logic (lines 34-38):

```rust
    if chat.is_newsletter {
        spans.push(Span::styled("[NL]", Style::default().fg(Color::Cyan)));
    } else {
        spans.push(Span::styled(tag, Style::default().fg(Color::DarkGray)));
    }
```

- [ ] **Step 1: Add `[GP]` branch**

Change the tag block to:

```rust
    if chat.is_newsletter {
        spans.push(Span::styled("[NL]", Style::default().fg(Color::Cyan)));
    } else if chat.is_group {
        spans.push(Span::styled("[GP]", Style::default().fg(Color::Magenta)));
    } else {
        spans.push(Span::styled(tag, Style::default().fg(Color::DarkGray)));
    }
```

- [ ] **Step 2: Run tests**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && cargo test 2>&1
```

Expected: `test result: ok. 22 passed`

- [ ] **Step 3: Commit**

```bash
git add src/tui/widgets/chat_list.rs
git commit -m "feat: show [GP] in magenta for group chats in chat list"
```

---

## Chunk 2: Right-aligned outgoing messages

### Task 2: Right-align outgoing message header and content in the message view

**Files:**
- Modify: `src/tui/widgets/message_view.rs:1-8` (imports)
- Modify: `src/tui/widgets/message_view.rs:126-159` (message rendering loop body)

#### Background

ratatui 0.29 `Line` supports `.alignment(Alignment::Right)`. When applied to a `Line`, ratatui right-justifies it within the available width. This is the correct API — no manual space-padding needed.

For outgoing messages the header order is reversed from `"sender  time"` to `"time  sender"` so it reads naturally when pushed to the right edge.

#### Step-by-step

- [ ] **Step 4: Add `Alignment` to imports**

The current import block (lines 1-8):

```rust
use chrono::Local;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
```

Change to:

```rust
use chrono::Local;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
```

- [ ] **Step 5: Right-align outgoing header**

The current header block (lines 126-134):

```rust
        let header = Line::from(vec![
            Span::styled(
                format!("{} ", msg.sender),
                Style::default().fg(sender_color),
            ),
            Span::styled(time, Style::default().fg(Color::DarkGray)),
        ]);

        lines.push(header);
```

Change to:

```rust
        let header = if msg.is_outgoing {
            // Right-aligned: "10:02  You" pushed to the right edge
            Line::from(vec![
                Span::styled(time, Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("  {}", msg.sender),
                    Style::default().fg(sender_color),
                ),
            ])
            .alignment(Alignment::Right)
        } else {
            // Left-aligned: "You 10:02" (unchanged)
            Line::from(vec![
                Span::styled(
                    format!("{} ", msg.sender),
                    Style::default().fg(sender_color),
                ),
                Span::styled(time, Style::default().fg(Color::DarkGray)),
            ])
        };

        lines.push(header);
```

- [ ] **Step 6: Right-align outgoing content lines**

The current content rendering (lines 139-160, through the closing `}` of the `for` loop body):

```rust
        for text_line in msg.content.as_text().split('\n') {
            if !text_line.contains("http://") && !text_line.contains("https://") {
                // Fast path: no URL prefix — single span, no allocation
                lines.push(Line::from(Span::styled(
                    text_line.to_string(),
                    Style::default().fg(msg_color),
                )));
                continue;
            }
            let spans: Vec<Span> = split_line_with_urls(text_line)
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
```

Change to:

```rust
        for text_line in msg.content.as_text().split('\n') {
            let line = if !text_line.contains("http://") && !text_line.contains("https://") {
                // Fast path: no URL prefix — single span, no allocation
                Line::from(Span::styled(
                    text_line.to_string(),
                    Style::default().fg(msg_color),
                ))
            } else {
                let spans: Vec<Span> = split_line_with_urls(text_line)
                    .into_iter()
                    .map(|(seg, is_url)| {
                        if is_url {
                            Span::styled(seg.to_string(), url_style)
                        } else {
                            Span::styled(seg.to_string(), Style::default().fg(msg_color))
                        }
                    })
                    .collect();
                Line::from(spans)
            };
            if msg.is_outgoing {
                lines.push(line.alignment(Alignment::Right));
            } else {
                lines.push(line);
            }
        }
```

- [ ] **Step 7: Run tests**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && cargo test 2>&1
```

Expected: `test result: ok. 22 passed`

- [ ] **Step 8: Smoke test**

```bash
cargo run
```

Verify visually:
- Outgoing messages: header shows `"HH:MM  You"` right-aligned at the right edge; message text is right-aligned
- Incoming messages: unchanged (left-aligned, sender then time)
- Group chat entries in the chat list show `[GP]` in magenta instead of `[WA]`/`[TG]`
- Newsletter entries still show `[NL]` in cyan

**Known limitation:** ratatui word-wraps long lines before applying alignment. If an outgoing message is wide enough to wrap, only the first visual sub-line will be right-aligned; continuation lines will appear left-aligned. This is acceptable for typical short chat messages.

- [ ] **Step 9: Commit**

```bash
git add src/tui/widgets/message_view.rs
git commit -m "feat: right-align outgoing messages in message view"
```
