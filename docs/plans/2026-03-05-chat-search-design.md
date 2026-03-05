# Chat Search Design

**Date:** 2026-03-05
**Feature:** `/` shortcut to fuzzy-find chats by display name, Enter to navigate + enter insert mode

## Overview

Press `/` in Normal mode to open a fuzzy-search popup over the chat list. Type to filter; the overlay shows the top 5 matching chats. Press Enter to jump to the selected chat and enter Insert (Editing) mode. Press Esc to cancel.

## Section 1 — State & Data Model

New `InputMode::Searching` variant added to the existing enum.

New struct on `AppState`:
```rust
pub struct SearchState {
    pub query: String,
    pub results: Vec<usize>,  // indices into state.chats (top 5)
    pub selected: usize,      // currently highlighted result
}
```

`AppState` gains `pub search_state: Option<SearchState>` — `None` when not active.

**Fuzzy match algorithm:** all chars of `query` must appear in order (case-insensitive) in `display_name ?? name`. Results scored by match span compactness (smaller span = better). Top 5 returned. No new crate required.

## Section 2 — Actions & Keybindings

New `Action` variants:
```rust
OpenSearch,
SearchInput(KeyEvent),
SearchNext,
SearchPrev,
SearchConfirm,
SearchClose,
```

- Normal mode: `'/'` → `OpenSearch`
- New `map_search_mode`:
  - `j` / `↓` → `SearchNext`
  - `k` / `↑` → `SearchPrev`
  - `Enter` → `SearchConfirm`
  - `Esc` → `SearchClose`
  - everything else → `SearchInput(key)`

Status bar hint:
```
Type to filter | j/k:Navigate | Enter:Open+Insert | Esc:Cancel
```

## Section 3 — Overlay Widget

New file: `src/tui/widgets/search_overlay.rs`

Popup rendered over the chat list area:
```
┌─ Find Chat ──────────────────┐
│ / query_text▌                │
│──────────────────────────────│
│ ▶ [WA] xin wei               │  ← selected (blue bg, white fg)
│   [WA] wei ming              │
│   [WA] xin xin               │
└──────────────────────────────┘
```

- Width: full chat list panel width
- Height: 2 (borders) + 1 (query line) + 1 (divider) + up to 5 results
- Empty query → results area hidden
- Highlight style matches existing chat selection (blue bg, white fg, bold)

`render.rs` renders this overlay last (on top) when `input_mode == Searching`.

## Section 4 — App Handler Logic

- **`OpenSearch`** — `input_mode = Searching`, `search_state = Some(SearchState::default())`
- **`SearchInput(key)`** — update `query` (backspace removes last char, printable appends), recompute top-5 results, clamp `selected`
- **`SearchNext`/`SearchPrev`** — move `selected`, wrapping within results
- **`SearchConfirm`** — if results non-empty: select chat, load messages, capture new message count, clear unread, send read receipts, set `input_mode = Editing`, clear search state, refresh title
- **`SearchClose`** — clear search state, `input_mode = Normal`

## Files Changed

| File | Change |
|------|--------|
| `src/tui/app_state.rs` | Add `InputMode::Searching`, `SearchState`, `search_state` field |
| `src/tui/keybindings.rs` | Add 6 action variants, `'/'` in normal map, `map_search_mode` |
| `src/tui/widgets/search_overlay.rs` | New file — popup widget |
| `src/tui/widgets/mod.rs` | `pub mod search_overlay` |
| `src/tui/render.rs` | Render search overlay when Searching |
| `src/tui/widgets/status_bar.rs` | Add Searching hint |
| `src/tui/widgets/input_bar.rs` | Add `InputMode::Searching` arm ("SEARCH" tag) |
| `src/app.rs` | Handle 6 new actions |
