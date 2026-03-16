# Typing Indicator — Running Light Animation

**Date:** 2026-03-16
**Status:** Approved

## Goal

Replace the current single-dot green↔gray blink with a 3-dot running-light (chaser) animation: three `●` dots rendered inline, with one lit green at a time sweeping left→right.

## Visual

```
phase=0 → ●○○  Alice  typing
phase=1 → ○●○  Alice  typing
phase=2 → ○○●  Alice  typing
```

- Lit dot: `Color::Green`
- Dim dots: `Color::DarkGray`
- Character: `●` (U+25CF, same as current)
- Cycle time: 2 ticks per phase step × 250ms tick = 500ms/step → ~1.5s full sweep

## Data Model Change

`blink_phase` changes type from `bool` to `u8` with values `0`, `1`, `2`.

**`src/tui/app_state.rs`:**
```rust
// before
pub blink_phase: bool,

// after
/// Running-light phase: 0=first dot lit, 1=middle, 2=last. Cycles every 2 ticks (~500ms/step).
pub blink_phase: u8,
```

Initialization: `blink_phase: 0`

## Tick Handler Change

**`src/app.rs`** — in `handle_tick`, replace the toggle:
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

## Widget Change

**`src/tui/widgets/chat_list.rs`:**

`make_item` signature:
```rust
// before
fn make_item(chat: &UnifiedChat, is_selected: bool, typing_blink: Option<bool>) -> ListItem<'static>

// after
fn make_item(chat: &UnifiedChat, is_selected: bool, typing_blink: Option<u8>) -> ListItem<'static>
```

Typing branch of `spans` construction:
```rust
// before
Span::styled("● ", Style::default().fg(dot_color)),
Span::styled(name, ...),
Span::styled(" typing", ...),

// after — 3 dots, each colored independently
let dot = |i: u8| {
    let color = if phase == i { Color::Green } else { Color::DarkGray };
    Span::styled("● ", Style::default().fg(color))
};
// spans:
dot(0),
dot(1),
dot(2),
Span::styled(name, ...),
Span::styled(" typing", ...),
```

`render_chat_list` signature — `blink_phase: bool` → `blink_phase: u8` (parameter type only; the call sites `typing_states.get(&chat.id).map(|_| blink_phase)` are unchanged in structure).

## Call Site Change

**`src/tui/render.rs`** — no change needed. `state.blink_phase` is passed through as-is; the type flows automatically.

The three `make_item` call sites in `chat_list.rs` that compute:
```rust
let blink = typing_states.get(&chat.id).map(|_| blink_phase);
```
remain structurally identical — the closure is unchanged, the type inference updates automatically.

## Testing

Update the existing unit test in `app_state.rs`:
```rust
// before
assert!(!state.blink_phase);

// after
assert_eq!(state.blink_phase, 0);
```

No other test changes required. All 82 existing tests must still pass.

## Notes

- **Width:** Each dot is `"● "` (2 chars), so 3 dots = 6 chars total — 4 chars more than before. In narrow panes the chat name may truncate slightly more. Acceptable trade-off.
- **Doc comment:** Update the `blink_phase` field comment in `app_state.rs` from `"green↔gray blink animation"` to reflect the 3-phase running-light behaviour.
- **Highlight style fg:** `render_chat_list` must NOT set `.fg(Color::White)` on the highlight style. ratatui applies the highlight style over span-level colors; setting a global `fg` would override the per-dot green color, making the chaser invisible on selected rows. The `▶` selector and blue background are sufficient to communicate selection.

## Out of Scope

- Changing the dot character to a larger glyph
- Per-chat independent phase offsets
- Directional reverse sweep (right→left on alternate cycles)
