# Typing Indicator Running Light Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the single green↔gray blinking dot with a 3-dot running-light (chaser) animation where one green dot sweeps left→right across three dim dots.

**Architecture:** Change `blink_phase` from `bool` to `u8` (0/1/2) throughout the codebase. The tick handler advances the phase by 1 mod 3 every 2 ticks. The chat list widget renders three `●` spans, coloring the one at index `phase` green and the others `DarkGray`.

**Tech Stack:** Rust, ratatui 0.29, tokio (250ms tick rate)

---

## Chunk 1: All changes (single task — small scope)

### Task 1: Upgrade blink_phase to u8 and render 3-dot chaser

**Files:**
- Modify: `src/tui/app_state.rs:361-362` (field type + doc comment), `398` (init), `581` (test)
- Modify: `src/app.rs:357-359` (tick handler toggle)
- Modify: `src/tui/widgets/chat_list.rs:14` (signature), `56-76` (spans), `88` (render_chat_list param)

> `src/tui/render.rs` needs no change — it passes `state.blink_phase` by value; the type flows automatically.

---

- [ ] **Step 1.1: Update `blink_phase` field in `AppState`**

Open `src/tui/app_state.rs`. Make three edits:

**Edit 1** — field declaration (line 361–362):
```rust
// before
    /// Toggled every 2 ticks (~500ms) to drive the green↔gray blink animation.
    pub blink_phase: bool,

// after
    /// Running-light phase: 0=first dot lit, 1=middle, 2=last. Cycles every 2 ticks (~500ms/step).
    pub blink_phase: u8,
```

**Edit 2** — initializer (line 398):
```rust
// before
            blink_phase: false,

// after
            blink_phase: 0,
```

**Edit 3** — existing unit test assertion (line 581):
```rust
// before
        assert!(!state.blink_phase);

// after
        assert_eq!(state.blink_phase, 0);
```

---

- [ ] **Step 1.2: Update tick handler in `App`**

Open `src/app.rs` lines 357–359. Replace the toggle:

```rust
// before
        if self.tick_count % 2 == 0 {
            self.state.blink_phase = !self.state.blink_phase;
        }

// after
        if self.tick_count % 2 == 0 {
            self.state.blink_phase = (self.state.blink_phase + 1) % 3;
        }
```

---

- [ ] **Step 1.3: Update `make_item` and `render_chat_list` in `chat_list.rs`**

Open `src/tui/widgets/chat_list.rs`. Make two edits:

**Edit 1** — `make_item` signature (line 14):
```rust
// before
fn make_item(chat: &UnifiedChat, is_selected: bool, typing_blink: Option<bool>) -> ListItem<'static> {

// after
fn make_item(chat: &UnifiedChat, is_selected: bool, typing_blink: Option<u8>) -> ListItem<'static> {
```

**Edit 2** — typing branch of `spans` (lines 56–66). Replace:
```rust
    let spans = if let Some(phase) = typing_blink {
        let dot_color = if phase { Color::Green } else { Color::DarkGray };
        vec![
            Span::raw(selector),
            Span::styled(pin_tag.to_string(), Style::default().fg(Color::Yellow)),
            platform_span,
            emoji_span,
            Span::styled("● ", Style::default().fg(dot_color)),
            Span::styled(name, Style::default().fg(name_color)),
            Span::styled(" typing", Style::default().fg(Color::DarkGray)),
        ]
```

with:
```rust
    let spans = if let Some(phase) = typing_blink {
        let dot = |i: u8| {
            let color = if phase == i { Color::Green } else { Color::DarkGray };
            Span::styled("● ", Style::default().fg(color))
        };
        vec![
            Span::raw(selector),
            Span::styled(pin_tag.to_string(), Style::default().fg(Color::Yellow)),
            platform_span,
            emoji_span,
            dot(0),
            dot(1),
            dot(2),
            Span::styled(name, Style::default().fg(name_color)),
            Span::styled(" typing", Style::default().fg(Color::DarkGray)),
        ]
```

**Edit 3** — `render_chat_list` parameter (line 88):
```rust
// before
    blink_phase: bool,

// after
    blink_phase: u8,
```

> The three `make_item` call sites (`typing_states.get(&chat.id).map(|_| blink_phase)`) need no changes — the closure body is unchanged, only the inferred type changes.

---

- [ ] **Step 1.4: Verify compilation**

```bash
cargo check 2>&1 | head -30
```

Expected: zero errors. There may be a warning about `dot` closure — that's fine.

---

- [ ] **Step 1.5: Run all tests**

```bash
cargo test 2>&1 | tail -10
```

Expected: `82 passed; 0 failed`

---

- [ ] **Step 1.6: Commit**

```bash
git add src/tui/app_state.rs src/app.rs src/tui/widgets/chat_list.rs
git commit -m "feat: replace blink dot with 3-dot running-light chaser animation"
```
