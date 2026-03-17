# btop UI Enhancements Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two parallel visual enhancements: colored-bar `┃` message bubbles in the message view, and a Braille activity sparkline column in the chat list.

**Architecture:** Feature F rewrites the gutter+header rendering in `message_view.rs` — incoming messages get a left purple `┃`, outgoing messages are right-padded with a cyan `┃` and delivery indicator. Because all lines are pre-wrapped before being pushed into the `Vec<Line>`, the `Wrap` widget option is removed so ratatui does not re-wrap them at render time. Feature C adds a new `activity.rs` storage module that queries 24h message counts per chat, caches them in `AppState`, and renders a 10-char Braille column in `chat_list.rs` when terminal width ≥ 100.

**Tech Stack:** Rust, ratatui 0.29, rusqlite, unicode-width crate (already in Cargo.toml)

**Spec:** `docs/superpowers/specs/2026-03-17-btop-ui-enhancements-design.md`

---

## Chunk 1: Feature F — Colored-Bar Message Bubbles

### Task 1: Delivery status helper

**Files:**
- Modify: `src/tui/widgets/message_view.rs`

- [ ] **Step 1: Write failing tests for `display_width_of_status`**

Add to the `#[cfg(test)]` block at the bottom of `src/tui/widgets/message_view.rs`:

```rust
#[test]
fn status_width_all_variants() {
    use crate::core::types::MessageStatus;
    assert_eq!(display_width_of_status(MessageStatus::Sending), 3);
    assert_eq!(display_width_of_status(MessageStatus::Sent), 1);
    assert_eq!(display_width_of_status(MessageStatus::Delivered), 2);
    assert_eq!(display_width_of_status(MessageStatus::Read), 2);
    assert_eq!(display_width_of_status(MessageStatus::Failed), 1);
}
```

- [ ] **Step 2: Run to confirm compile error (function missing)**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && cargo test status_width_all_variants -q 2>&1 | head -5
```

Expected: `error[E0425]: cannot find function \`display_width_of_status\``

- [ ] **Step 3: Add helpers**

Update the module-level import near the top of `message_view.rs`:

```rust
use crate::core::types::{MessageStatus, UnifiedMessage};
```

Then add these two functions just before `render_message_view` (before the `#[allow(clippy::too_many_arguments)]` attribute):

```rust
/// Returns the display column width of the delivery status indicator.
fn display_width_of_status(status: MessageStatus) -> usize {
    match status {
        MessageStatus::Sending => 3,   // "···"  (3 × U+00B7)
        MessageStatus::Sent => 1,      // "✓"
        MessageStatus::Delivered => 2, // "✓✓"
        MessageStatus::Read => 2,      // "✓✓"
        MessageStatus::Failed => 1,    // "✗"
    }
}

/// Returns the styled delivery-status span.
fn status_span(status: MessageStatus) -> Span<'static> {
    let (text, color) = match status {
        MessageStatus::Sending => ("···", Color::DarkGray),
        MessageStatus::Sent => ("✓", Color::DarkGray),
        MessageStatus::Delivered => ("✓✓", Color::DarkGray),
        MessageStatus::Read => ("✓✓", Color::Green),
        MessageStatus::Failed => ("✗", Color::Red),
    };
    Span::styled(text, Style::default().fg(color))
}
```

- [ ] **Step 4: Run tests**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && cargo test status_width_all_variants -q
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd /home/weibin/repo/ai/zero-drift-chat
git add src/tui/widgets/message_view.rs
git commit -m "$(cat <<'EOF'
feat: add display_width_of_status and status_span helpers

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `build_content_spans` helper + incoming/outgoing bubble rendering

**Files:**
- Modify: `src/tui/widgets/message_view.rs`

- [ ] **Step 1: Add `build_content_spans` helper**

Add this function after `split_line_with_urls` (after line 126):

```rust
/// Build content spans for a single pre-wrapped line with optional URL styling.
/// `is_selected`: adds blue background when true.
/// `text_color`: base message text color.
fn build_content_spans(line: &str, is_selected: bool, text_color: Color) -> Vec<Span<'static>> {
    let bg = if is_selected { Color::Blue } else { Color::Black };
    let url_style = Style::default()
        .fg(Color::LightBlue)
        .add_modifier(Modifier::UNDERLINED)
        .bg(bg);

    if !line.contains("http://") && !line.contains("https://") {
        return vec![Span::styled(
            line.to_string(),
            Style::default().fg(text_color).bg(bg),
        )];
    }

    split_line_with_urls(line)
        .into_iter()
        .map(|(seg, is_url)| {
            if is_url {
                Span::styled(seg.to_string(), url_style)
            } else {
                Span::styled(seg.to_string(), Style::default().fg(text_color).bg(bg))
            }
        })
        .collect()
}
```

Note: uses `Color::Black` as the non-selected background (matches terminal default).

- [ ] **Step 2: Replace `render_message_view` message loop**

Replace the entire `for (i, msg) in messages.iter().enumerate() {` loop (lines 178–332) with:

```rust
    for (i, msg) in messages.iter().enumerate() {
        // Insert "─── N new ───" separator before first new message
        if Some(i) == new_start_idx {
            let content_width = area.width.saturating_sub(2) as usize;
            let label = format!(" {} new ", new_message_count);
            let dashes = content_width.saturating_sub(label.len());
            let left = dashes / 2;
            let right = dashes - left;
            let separator = format!("{}{}{}", "─".repeat(left), label, "─".repeat(right));
            lines.push(Line::styled(separator, Style::default().fg(Color::Yellow)));
        }

        let time = msg
            .timestamp
            .with_timezone(&Local)
            .format("%H:%M")
            .to_string();
        let is_selected = selected_message_idx == Some(i);

        // Group boundary: direction flip, different sender, or >5min gap
        let prev = if i > 0 { messages.get(i - 1) } else { None };
        let is_group_start = match prev {
            None => true,
            Some(p) => {
                p.is_outgoing != msg.is_outgoing
                    || p.sender != msg.sender
                    || (msg.timestamp - p.timestamp).num_seconds().abs() > 300
            }
        };

        // Blank separator between groups (not before the very first message)
        if is_group_start && i > 0 {
            lines.push(Line::from(""));
        }

        let area_w = area.width.saturating_sub(2) as usize;
        let sender_display = if msg.sender.is_empty() {
            "(unknown)".to_string()
        } else {
            msg.sender.clone()
        };

        if msg.is_outgoing {
            // ── Outgoing: right-aligned, cyan ┃ ──────────────────────────────
            let name = sender_display.clone();
            if is_group_start {
                let header = format!("{} {}", name, time);
                let pad = area_w.saturating_sub(header.len());
                let header_line = if is_selected {
                    Line::from(vec![
                        Span::raw(" ".repeat(pad)),
                        Span::styled(name.clone(), Style::default().bg(Color::Blue).fg(Color::Cyan)),
                        Span::styled(format!(" {}", time), Style::default().bg(Color::Blue).fg(Color::DarkGray)),
                        Span::styled(" ▐", Style::default().fg(Color::Cyan).bg(Color::Blue)),
                    ])
                } else {
                    Line::from(vec![
                        Span::raw(" ".repeat(pad)),
                        Span::styled(name.clone(), Style::default().fg(Color::Cyan)),
                        Span::styled(format!(" {}", time), Style::default().fg(Color::DarkGray)),
                    ])
                };
                lines.push(header_line);
            }

            let max_self_w = (area_w * 2 / 3).max(20);
            let content_text = msg.content.as_text().to_string();
            let mut all_wrapped: Vec<String> = Vec::new();
            for original_line in content_text.split('\n') {
                all_wrapped.extend(wrap_to_width(original_line, max_self_w));
            }
            let total = all_wrapped.len();
            for (li, text_line) in all_wrapped.iter().enumerate() {
                let is_last = li == total - 1;
                let line_w = UnicodeWidthStr::width(text_line.as_str());
                let mut spans: Vec<Span> = build_content_spans(text_line, is_selected, Color::White);
                if is_last {
                    let status_w = display_width_of_status(msg.status);
                    let pad = area_w.saturating_sub(line_w + 2 + 1 + status_w);
                    let mut row: Vec<Span> = vec![Span::raw(" ".repeat(pad))];
                    row.append(&mut spans);
                    row.push(Span::styled(" ┃", Style::default().fg(Color::Cyan).bg(if is_selected { Color::Blue } else { Color::Black })));
                    row.push(Span::raw(" "));
                    row.push({
                        let mut s = status_span(msg.status);
                        if is_selected { s = s.patch_style(Style::default().bg(Color::Blue)); }
                        s
                    });
                    lines.push(Line::from(row));
                } else {
                    let pad = area_w.saturating_sub(line_w + 2);
                    let mut row: Vec<Span> = vec![Span::raw(" ".repeat(pad))];
                    row.append(&mut spans);
                    row.push(Span::styled(" ┃", Style::default().fg(Color::Cyan).bg(if is_selected { Color::Blue } else { Color::Black })));
                    lines.push(Line::from(row));
                }
            }
        } else {
            // ── Incoming: left-aligned, purple ┃ ─────────────────────────────
            let is_new = new_start_idx.map(|s| i >= s).unwrap_or(false);
            let name_color = if is_new { Color::Yellow } else { Color::Magenta };
            let bar_color = if is_new { Color::Yellow } else { Color::Magenta };
            let msg_color = if is_new { Color::White } else { Color::Gray };

            if is_group_start {
                let header_line = if is_selected {
                    Line::from(vec![
                        Span::styled("▌ ", Style::default().fg(Color::Cyan).bg(Color::Blue)),
                        Span::styled(sender_display.clone(), Style::default().fg(name_color).bg(Color::Blue)),
                        Span::styled(format!(" {}", time), Style::default().fg(Color::DarkGray).bg(Color::Blue)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled("┃ ", Style::default().fg(bar_color)),
                        Span::styled(sender_display.clone(), Style::default().fg(name_color)),
                        Span::styled(format!(" {}", time), Style::default().fg(Color::DarkGray)),
                    ])
                };
                lines.push(header_line);
            }

            let content_w = area_w.saturating_sub(2); // "┃ " = 2 cols
            let content_text = msg.content.as_text().to_string();
            for original_line in content_text.split('\n') {
                for text_line in wrap_to_width(original_line, content_w) {
                    let bar = if is_selected {
                        Span::styled("▌ ", Style::default().fg(Color::Cyan).bg(Color::Blue))
                    } else {
                        Span::styled("┃ ", Style::default().fg(bar_color))
                    };
                    let mut content_spans = build_content_spans(&text_line, is_selected, msg_color);
                    let mut row = vec![bar];
                    row.append(&mut content_spans);
                    lines.push(Line::from(row));
                }
            }
        }
    } // end message loop
```

- [ ] **Step 3: Remove `Wrap` from Paragraph — lines are pre-wrapped**

Lines are now explicitly pre-wrapped before being pushed into `lines`. The `Paragraph` widget must NOT re-wrap them. Replace the paragraph construction near the end of `render_message_view`:

**Old code:**
```rust
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((effective_scroll, 0));
```

**New code:**
```rust
    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((effective_scroll, 0));
```

Also remove `Wrap` from the ratatui imports at the top of the file:

```rust
// Remove Wrap from this import:
use ratatui::widgets::{Block, Borders, Paragraph};
```

- [ ] **Step 4: Build**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && cargo build 2>&1 | head -40
```

Expected: Clean build. Fix any borrow/type errors before continuing.

- [ ] **Step 5: Run all existing tests**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && cargo test -q 2>&1 | tail -10
```

Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
cd /home/weibin/repo/ai/zero-drift-chat
git add src/tui/widgets/message_view.rs
git commit -m "$(cat <<'EOF'
feat: replace gutter with colored bar bubble style (Feature F)

- Incoming: left purple ┃ with name/timestamp header on group start
- Outgoing: right-padded cyan ┃ with delivery status indicator
- Groups separated by blank line on sender/direction change or >5min gap
- Remove Wrap option from Paragraph since lines are pre-wrapped
- build_content_spans helper for URL-aware line rendering

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Fix scroll estimation

**Files:**
- Modify: `src/tui/widgets/message_view.rs`

Lines are now pre-wrapped and `Wrap` is removed from the Paragraph widget, so each `Line` in the vec is exactly one terminal row. The scroll estimation can be simplified to `lines.len()`.

- [ ] **Step 1: Write a test confirming pre-wrap line count**

Add to the `#[cfg(test)]` block:

```rust
#[test]
fn pre_wrap_one_line_per_terminal_row() {
    // 50-char string at max_w=40 wraps to 2 lines
    let text = "a".repeat(50);
    let wrapped = wrap_to_width(&text, 40);
    assert_eq!(wrapped.len(), 2);
    // 50-char string at max_w=60 stays 1 line
    let wrapped2 = wrap_to_width(&text, 60);
    assert_eq!(wrapped2.len(), 1);
}
```

Run: `cargo test pre_wrap_one_line -q` — Expected: PASS.

- [ ] **Step 2: Replace the scroll estimation block**

Replace lines 341–366 (the `// Auto-scroll: estimate total visual lines...` comment through `let auto_scroll = ...`) with:

```rust
    // Auto-scroll: each Line in `lines` is a pre-wrapped single terminal row
    // (Wrap is not set on the Paragraph, so no re-wrapping occurs at render time).
    let visible_height = area.height.saturating_sub(2) as usize;
    let total_lines = lines.len();
    let auto_scroll = if total_lines > visible_height {
        (total_lines - visible_height) as u16
    } else {
        0
    };
```

- [ ] **Step 3: Build and run tests**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && cargo test -q 2>&1 | tail -5
```

Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
cd /home/weibin/repo/ai/zero-drift-chat
git add src/tui/widgets/message_view.rs
git commit -m "$(cat <<'EOF'
feat: simplify scroll estimation for pre-wrapped bubble lines

Each Line is now exactly one terminal row (Wrap removed),
so scroll total = lines.len() directly.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Chunk 2: Feature C — Activity Graph Column

### Task 4: Braille encoder and SQL query

**Files:**
- Create: `src/storage/activity.rs`
- Modify: `src/storage/mod.rs`

- [ ] **Step 1: Check `src/storage/mod.rs` for existing `pub mod` declarations**

```bash
cat /home/weibin/repo/ai/zero-drift-chat/src/storage/mod.rs
```

Note the existing module names; you will add `pub mod activity;` alongside them.

- [ ] **Step 2: Create `src/storage/activity.rs` with tests**

```rust
// src/storage/activity.rs
use std::collections::HashMap;

use crate::storage::db::Database;

/// Encode a 24-hour hourly bucket array into a 10-character Braille sparkline.
/// `array[23]` = current hour, `array[0]` = 23 hours ago.
/// Renders the most recent 10 hours (indices 14..=23).
pub fn encode_braille(array: &[u32; 24]) -> String {
    const BRAILLE: [char; 9] = ['⠀', '⣀', '⣄', '⣆', '⣇', '⣧', '⣷', '⣾', '⣿'];
    let slice = &array[14..]; // 10 elements: indices 14..=23
    let max_val = *slice.iter().max().unwrap_or(&0);
    if max_val == 0 {
        return BRAILLE[0].to_string().repeat(10);
    }
    slice
        .iter()
        .map(|&v| BRAILLE[((v * 8) / max_val).min(8) as usize])
        .collect()
}

/// Query 24-hour message activity bucketed by hour for a list of chat IDs.
/// Returns chat_id → [u32; 24] where index 23 = current hour, index 0 = 23 hours ago.
/// Returns an empty map if `chat_ids` is empty.
///
/// Note: slot 23 corresponds to the current clock hour. A new message arriving just
/// after an hour boundary will be counted in slot 23 until the next full refresh
/// (~5 min). This ±1 bucket inaccuracy is acceptable per spec.
pub fn query_activity_24h(db: &Database, chat_ids: &[&str]) -> HashMap<String, [u32; 24]> {
    if chat_ids.is_empty() {
        return HashMap::new();
    }

    let placeholders: String = chat_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");

    // strftime('%s', ...) has broader SQLite compatibility than unixepoch()
    // (works from SQLite 3.8+, whereas unixepoch() requires 3.38+).
    // Timestamps are stored as RFC 3339 strings.
    let sql = format!(
        "SELECT chat_id,
                CAST((CAST(strftime('%s', 'now') AS INTEGER)
                      - CAST(strftime('%s', timestamp) AS INTEGER)) / 3600 AS INTEGER) AS hours_ago,
                COUNT(*) AS cnt
         FROM messages
         WHERE CAST(strftime('%s', timestamp) AS INTEGER)
               > CAST(strftime('%s', 'now') AS INTEGER) - 86400
           AND chat_id IN ({placeholders})
         GROUP BY chat_id, hours_ago"
    );

    let mut result: HashMap<String, [u32; 24]> = HashMap::new();

    let query_result = db.conn.prepare(&sql).and_then(|mut stmt| {
        let params: Vec<&dyn rusqlite::ToSql> =
            chat_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(params.as_slice(), |row| {
            let chat_id: String = row.get(0)?;
            let hours_ago: i64 = row.get(1)?;
            let cnt: u32 = row.get(2)?;
            Ok((chat_id, hours_ago, cnt))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
    });

    match query_result {
        Ok(triples) => {
            for (chat_id, hours_ago, cnt) in triples {
                if hours_ago < 0 || hours_ago > 23 {
                    continue;
                }
                let slot = 23 - hours_ago as usize;
                result.entry(chat_id).or_insert([0u32; 24])[slot] = cnt;
            }
        }
        Err(e) => {
            tracing::warn!("activity query failed: {}", e);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::{encode_braille, query_activity_24h};
    use crate::storage::db::Database;

    #[test]
    fn all_zero_returns_blank_braille() {
        let arr = [0u32; 24];
        let result = encode_braille(&arr);
        assert_eq!(result.chars().count(), 10);
        assert!(result.chars().all(|c| c == '⠀'));
    }

    #[test]
    fn single_spike_at_current_hour() {
        let mut arr = [0u32; 24];
        arr[23] = 100;
        let result = encode_braille(&arr);
        let chars: Vec<char> = result.chars().collect();
        assert_eq!(chars.len(), 10);
        assert_eq!(*chars.last().unwrap(), '⣿');
        for c in &chars[..9] {
            assert_eq!(*c, '⠀');
        }
    }

    #[test]
    fn uniform_distribution_all_same_level() {
        let mut arr = [0u32; 24];
        for i in 14..24 {
            arr[i] = 50;
        }
        let result = encode_braille(&arr);
        let chars: Vec<char> = result.chars().collect();
        let first = chars[0];
        assert!(chars.iter().all(|&c| c == first));
        assert_ne!(first, '⠀');
    }

    #[test]
    fn indices_outside_14_23_are_ignored() {
        // Spike at index 0 is outside the [14..] slice, should not affect output
        let mut arr = [0u32; 24];
        arr[0] = 9999;
        arr[23] = 1;
        let chars: Vec<char> = encode_braille(&arr).chars().collect();
        assert_eq!(*chars.last().unwrap(), '⣿');
        for c in &chars[..9] {
            assert_eq!(*c, '⠀');
        }
    }

    #[test]
    fn max_val_zero_no_panic() {
        let arr = [0u32; 24];
        let r = encode_braille(&arr);
        assert_eq!(r.chars().count(), 10);
    }

    #[test]
    fn empty_chat_ids_returns_empty_map() {
        let db = Database::open_in_memory().expect("in-memory db");
        let result = query_activity_24h(&db, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn query_with_no_matching_messages_returns_empty() {
        let db = Database::open_in_memory().expect("in-memory db");
        let result = query_activity_24h(&db, &["chat_nonexistent"]);
        assert!(result.is_empty());
    }
}
```

- [ ] **Step 3: Register the module**

Open `src/storage/mod.rs` and add:

```rust
pub mod activity;
```

alongside the existing `pub mod` declarations found in Step 1.

- [ ] **Step 4: Run tests**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && cargo test storage::activity:: -q
```

Expected: All 7 tests pass.

- [ ] **Step 5: Commit**

```bash
cd /home/weibin/repo/ai/zero-drift-chat
git add src/storage/activity.rs src/storage/mod.rs
git commit -m "$(cat <<'EOF'
feat: add activity Braille encoder and 24h SQL query (Feature C)

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: AppState — add activity cache fields

**Files:**
- Modify: `src/tui/app_state.rs`

- [ ] **Step 1: Add fields to `AppState` struct**

In `src/tui/app_state.rs`, find `pub blink_phase: u8,` (line ~362). Add after it:

```rust
    /// Per-chat 24h activity cache: chat_id → [u32; 24].
    /// Index 23 = current hour, index 0 = 23 hours ago.
    pub activity_cache: std::collections::HashMap<String, [u32; 24]>,
    /// `tick_count` value when the last full SQL activity refresh ran.
    pub activity_last_refresh_tick: u64,
```

- [ ] **Step 2: Initialize in `AppState::new()`**

In `AppState::new()`, add after `blink_phase: 0,`:

```rust
            activity_cache: std::collections::HashMap::new(),
            activity_last_refresh_tick: 0,
```

- [ ] **Step 3: Build**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && cargo build -q 2>&1 | head -10
```

Expected: Clean.

- [ ] **Step 4: Commit**

```bash
cd /home/weibin/repo/ai/zero-drift-chat
git add src/tui/app_state.rs
git commit -m "$(cat <<'EOF'
feat: add activity_cache fields to AppState

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: Wire activity refresh in `app.rs`

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add `refresh_activity_cache` method**

Add this method to `impl App` (alongside `handle_tick`):

```rust
    fn refresh_activity_cache(&mut self) {
        let chat_ids: Vec<String> = self.state.chats.iter().map(|c| c.id.clone()).collect();
        let id_refs: Vec<&str> = chat_ids.iter().map(|s| s.as_str()).collect();
        let cache = crate::storage::activity::query_activity_24h(&self.db, &id_refs);
        self.state.activity_cache = cache;
    }
```

- [ ] **Step 2: Call on startup**

In `run()`, after `self.load_selected_chat_messages();` (~line 203), add:

```rust
        // Populate activity cache before first render
        self.refresh_activity_cache();
        self.state.activity_last_refresh_tick = 0;
```

- [ ] **Step 3: Periodic refresh in `handle_tick`**

In `handle_tick`, after `self.tick_count += 1;` (~line 354), add:

```rust
        // Refresh activity cache every 1200 ticks (~5 minutes at 250ms/tick)
        if self.tick_count.saturating_sub(self.state.activity_last_refresh_tick) >= 1200 {
            self.state.activity_last_refresh_tick = self.tick_count;
            self.refresh_activity_cache();
        }
```

- [ ] **Step 4: Increment on new message**

In the `ProviderEvent::NewMessage(msg)` handler (~line 367), after `self.db.insert_message(&msg)`, add:

```rust
                    // Increment activity cache for the current hour bucket (slot 23).
                    // ±1 bucket error possible within 5 min of an hour boundary;
                    // the next full refresh corrects it automatically.
                    self.state.activity_cache
                        .entry(msg.chat_id.clone())
                        .or_insert([0u32; 24])[23] += 1;
```

- [ ] **Step 5: Build**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && cargo build 2>&1 | head -20
```

Expected: Clean. If `run()` already has `&mut self`, the startup call compiles without changes.

- [ ] **Step 6: Commit**

```bash
cd /home/weibin/repo/ai/zero-drift-chat
git add src/app.rs
git commit -m "$(cat <<'EOF'
feat: wire activity cache refresh on startup, tick, and new message

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: Chat list — activity graph column

**Files:**
- Modify: `src/tui/widgets/chat_list.rs`
- Modify: `src/tui/render.rs`

- [ ] **Step 1: Add `Paragraph` to imports in `chat_list.rs`**

Update the widgets import line (line 7):

```rust
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
```

- [ ] **Step 2: Add `activity_cache` parameter to `render_chat_list`**

Change the function signature (line 85) to:

```rust
pub fn render_chat_list(
    f: &mut Frame,
    area: Rect,
    chats: &[UnifiedChat],
    list_state: &mut ListState,
    active_panel: ActivePanel,
    input_mode: InputMode,
    typing_states: &HashMap<String, TypingInfo>,
    blink_phase: u8,
    activity_cache: &HashMap<String, [u32; 24]>,
) {
```

- [ ] **Step 3: Add horizontal split at the top of `render_chat_list`**

Add this block immediately after the opening `{` of `render_chat_list`, before `let border_color = ...`:

```rust
    // Wide-screen: split off a 12-col graph column on the right
    let (list_area, graph_area_opt) = if area.width >= 100 {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(area.width.saturating_sub(12)),
                Constraint::Length(12),
            ])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };
```

- [ ] **Step 4: Replace `area` with `list_area` throughout the function body**

The function body uses `area` in four places after the split. Replace each:
- `let block = Block::default()...` — no change (block is rendered next)
- `let inner = block.inner(area)` → `let inner = block.inner(list_area)`
- `f.render_widget(block, area)` → `f.render_widget(block, list_area)`
- The `f.render_stateful_widget(list, inner, list_state)` call inside the `pinned_count == 0` branch — replace the `return` after it with a `// fall through to graph column` comment and move graph rendering below the if/else. See Step 5.

- [ ] **Step 5: Restructure the `pinned_count == 0` early return**

The original code returns early when there are no pinned chats (line 152). This prevents graph column rendering. Replace the early return with a local variable pattern:

```rust
    if pinned_count == 0 {
        // No pinned chats — plain scrollable list
        let items: Vec<ListItem> = chats
            .iter()
            .enumerate()
            .map(|(i, chat)| {
                let blink = typing_states.get(&chat.id).map(|_| blink_phase);
                make_item(chat, i == selected, blink)
            })
            .collect();
        let list = List::new(items).highlight_style(highlight);
        f.render_stateful_widget(list, inner, list_state);
        // Do NOT return — fall through to render graph column below
    } else {
        // Split inner area: fixed pinned section on top, scrollable unpinned below
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(pinned_count as u16), Constraint::Min(0)])
            .split(inner);

        // --- Pinned section ---
        let pinned_items: Vec<ListItem> = chats
            .iter()
            .filter(|c| c.is_pinned)
            .enumerate()
            .map(|(i, chat)| {
                let blink = typing_states.get(&chat.id).map(|_| blink_phase);
                make_item(chat, selected < pinned_count && i == selected, blink)
            })
            .collect();
        let mut pinned_state = ListState::default();
        if selected < pinned_count {
            pinned_state.select(Some(selected));
        }
        let pinned_list = List::new(pinned_items).highlight_style(highlight);
        f.render_stateful_widget(pinned_list, sections[0], &mut pinned_state);

        // --- Unpinned section ---
        let unpinned_items: Vec<ListItem> = chats
            .iter()
            .filter(|c| !c.is_pinned)
            .enumerate()
            .map(|(i, chat)| {
                let blink = typing_states.get(&chat.id).map(|_| blink_phase);
                make_item(
                    chat,
                    selected >= pinned_count && i == selected - pinned_count,
                    blink,
                )
            })
            .collect();
        let mut unpinned_state = ListState::default();
        if selected >= pinned_count {
            unpinned_state.select(Some(selected - pinned_count));
        }
        let unpinned_list = List::new(unpinned_items).highlight_style(highlight);
        f.render_stateful_widget(unpinned_list, sections[1], &mut unpinned_state);
    }

    // Render activity graph column if wide enough
    if let Some(graph_area) = graph_area_opt {
        use crate::storage::activity::encode_braille;

        let mut graph_lines: Vec<ratatui::text::Line> = vec![
            ratatui::text::Line::from(vec![
                Span::styled(" 24h       ", Style::default().fg(Color::DarkGray)),
            ]),
        ];
        for chat in chats.iter() {
            let arr = activity_cache
                .get(&chat.id)
                .copied()
                .unwrap_or([0u32; 24]);
            let braille = encode_braille(&arr);
            let color = if chat.unread_count > 0 {
                Color::Green
            } else {
                Color::DarkGray
            };
            graph_lines.push(ratatui::text::Line::from(vec![
                Span::raw(" "),
                Span::styled(braille, Style::default().fg(color)),
                Span::raw(" "),
            ]));
        }

        let graph_widget = Paragraph::new(graph_lines);
        f.render_widget(graph_widget, graph_area);
    }
```

- [ ] **Step 6: Update the call site in `render.rs`**

In `src/tui/render.rs`, find the `chat_list::render_chat_list(...)` call (lines 56–65). Add the new argument:

```rust
    chat_list::render_chat_list(
        f,
        chat_list_area,
        &state.chats,
        &mut state.chat_list_state,
        state.active_panel,
        state.input_mode,
        &state.typing_states,
        state.blink_phase,
        &state.activity_cache,   // ← new
    );
```

- [ ] **Step 7: Build**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && cargo build 2>&1 | head -40
```

Expected: Clean. Fix any import or borrow errors.

- [ ] **Step 8: Run all tests**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && cargo test -q 2>&1 | tail -10
```

Expected: All tests pass.

- [ ] **Step 9: Commit**

```bash
cd /home/weibin/repo/ai/zero-drift-chat
git add src/tui/widgets/chat_list.rs src/tui/render.rs
git commit -m "$(cat <<'EOF'
feat: add 24h activity graph column to chat list (Feature C)

Shows Braille sparkline column when terminal width >= 100 cols.
Column is borderless (12 cols: 1 pad + 10 Braille + 1 pad).
Graph aligns with both pinned and unpinned chat rows.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: Manual verification (Feature C)

- [ ] **Step 1: Verify app starts without crash**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && cargo run 2>/dev/null
```

Expected: App launches, no panic on startup.

- [ ] **Step 2: Test activity graph at wide terminal (≥100 cols)**

Resize terminal to ≥100 columns. Verify:
- A `24h` header row appears on the right of the chat list
- Each chat row shows a 10-char Braille sparkline next to it
- Chats with unread messages show green sparklines; others show DarkGray

- [ ] **Step 3: Test narrow terminal (<100 cols)**

Resize terminal to <100 columns. Verify:
- Activity graph column disappears
- Chat list fills full width as before

- [ ] **Step 4: Verify Feature F visuals**

While running with any terminal width, confirm:
- Incoming messages show purple `┃` on the left with name + timestamp header
- Outgoing messages are right-aligned with cyan `┃` and delivery status
- Consecutive same-sender messages share a single header
- A blank separator line appears between groups

- [ ] **Step 5: Check git log**

```bash
cd /home/weibin/repo/ai/zero-drift-chat && git log --oneline -10
```

Expected: All feature commits visible in order.
