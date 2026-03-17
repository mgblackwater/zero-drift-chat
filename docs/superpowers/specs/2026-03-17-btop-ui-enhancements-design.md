# btop-Inspired UI Enhancements Design

**Date:** 2026-03-17
**Status:** Approved
**Features:** Message bubble style (F) + Activity graph column (C)

---

## Overview

Two parallel visual enhancements inspired by btop's UI design philosophy:

1. **Colored-bar message bubbles** — replace the current `▌`/`▐` gutter with colored `┃` vertical bars that distinguish sender vs receiver, with right-aligned self-messages and delivery indicators.
2. **Activity graph column** — show a Braille sparkline of 24-hour message frequency per chat in a side column of the chat list, visible when terminal width ≥ 100 columns.

Both features are always-on (no toggle). Both are implemented in parallel.

---

## Feature F: Colored-Bar Message Bubbles

### Sender Detection

Use `msg.is_outgoing: bool` (present on `UnifiedMessage`) for left/right split. Use `msg.sender: String` for display name.

### Grouping Logic

Track `prev_is_outgoing: Option<bool>` and `prev_sender: Option<String>` while iterating messages. A **group boundary** occurs when either:
- `msg.is_outgoing != prev_is_outgoing` (direction flip), OR
- `msg.sender != prev_sender` (different sender in group chat), OR
- time gap > 5 minutes between consecutive messages

Insert one blank separator line at each group boundary. `prev_is_outgoing` drives the direction choice; `prev_sender` handles same-direction multi-sender groups. Both fields are necessary.

Name/timestamp header is rendered only on the first message of each group.

### Delivery Status Indicator

`UnifiedMessage` has `status: MessageStatus` with variants `Sending`, `Sent`, `Delivered`, `Read`, `Failed`.

| Status | Indicator | Color |
|--------|-----------|-------|
| `Sending` | `···` | DarkGray |
| `Sent` | `✓` | DarkGray |
| `Delivered` | `✓✓` | DarkGray |
| `Read` | `✓✓` | Green |
| `Failed` | `✗` | Red |

The indicator is appended **after** `┃` on the last line of every outgoing message, separated by one space: `Cyan("┃") + " " + colored_status`. The pad calculation for the last content line must reserve space for the status: `pad = area_w - line_w - 2 - 1 - status_w` where `2` = space+`┃`, `1` = separator space, `status_w` = display width of the status string. Non-last lines use `pad = area_w - line_w - 2`.

**Status display widths** (all characters have width 1 per `unicode-width` crate):

| Status | Indicator | `status_w` |
|--------|-----------|------------|
| `Sending` | `···` (3× U+00B7) | 3 |
| `Sent` | `✓` (U+2713) | 1 |
| `Delivered` | `✓✓` | 2 |
| `Read` | `✓✓` | 2 |
| `Failed` | `✗` (U+2717) | 1 |

`display_width_of_status(s: MessageStatus) -> usize` returns these values directly (no runtime Unicode measurement needed).

### Right-Aligned Layout

All arithmetic in `usize` (cast `area.width as usize` per existing pattern in the codebase).

```
area_w = area.width as usize
max_self_w = area_w * 2 / 3

// Wrap content
lines = wrap_to_width(&content, max_self_w)

// Header line (name + timestamp), right-aligned
header = format!("{name} {timestamp}")
header_pad = area_w.saturating_sub(header.len())
render: " ".repeat(header_pad) + Cyan(name) + " " + Gray(timestamp)

// Content lines
for (i, line) in lines.iter().enumerate() {
    is_last = i == lines.len() - 1
    line_w  = UnicodeWidthStr::width(line.as_str())
    if is_last {
        status_w = display_width_of_status(msg.status)
        pad = area_w.saturating_sub(line_w + 2 + 1 + status_w)
        render: " ".repeat(pad) + line + " " + Cyan("┃") + " " + colored_status
    } else {
        pad = area_w.saturating_sub(line_w + 2)
        render: " ".repeat(pad) + line + " " + Cyan("┃")
    }
}
```

### Scroll Estimation Fix

The existing estimation loop in `message_view.rs` uses a single `bubble_max_w`. Branch on `msg.is_outgoing`:

```rust
let max_w = if msg.is_outgoing {
    area.width as usize * 2 / 3
} else {
    area.width.saturating_sub(2) as usize
};
let wrapped_count = wrap_to_width(&line, max_w).len();
```

### Message Selection Mode

The existing code uses `▌` on the left for incoming selected messages and `▐` on the right for outgoing selected messages.

- **Incoming messages:** `▌` is a left-side gutter; the new `┃` is also left-side. In `MessageSelect` mode, render `▌` + blue background in place of `┃` for the selected message row. No structural change needed.
- **Outgoing messages:** `▐` is a right-side marker; the new `┃` is also right-side. In `MessageSelect` mode, render `▐` + blue background in place of `┃` for the selected message row.

Both selection markers are retained exactly as implemented today; only the non-selected gutter character changes (`▌`/`▐` → `┃`).

### File

`src/tui/widgets/message_view.rs` — replace gutter + `sender: content` rendering logic.

---

## Feature C: Activity Graph Column

### New File: `src/storage/activity.rs`

```rust
/// Returns message counts bucketed by hour for the past 24h.
/// Index 0 = 23 hours ago, index 23 = the current (most recent) hour.
pub fn query_activity_24h(
    db: &Database,
    chat_ids: &[&str],
) -> HashMap<String, [u32; 24]>
```

Note: use `Database` (the actual type from `src/storage/db.rs`).

**Handling variable-length IN clause:** rusqlite does not support binding a slice to `IN (?)`. Build the placeholder list dynamically:

```rust
let placeholders = chat_ids.iter().enumerate()
    .map(|(i, _)| format!("?{}", i + 1))
    .collect::<Vec<_>>()
    .join(", ");
let sql = format!(
    "SELECT chat_id, \
     CAST((unixepoch('now') - unixepoch(timestamp)) / 3600 AS INTEGER) AS hours_ago, \
     COUNT(*) AS cnt \
     FROM messages \
     WHERE unixepoch(timestamp) > unixepoch('now') - 86400 \
       AND chat_id IN ({placeholders}) \
     GROUP BY chat_id, hours_ago"
);
// bind chat_ids[0], chat_ids[1], ... positionally
```

Map result into `[u32; 24]`: `array[23 - hours_ago] = cnt`. Rows with `hours_ago > 23` are discarded.

**SQLite version requirement:** `unixepoch()` accepting RFC 3339 strings requires SQLite ≥ 3.38.0 (2022-02-22). If the bundled `rusqlite` uses an older SQLite (check via `rusqlite::version()`), use `strftime('%s', timestamp)` as a drop-in replacement with identical semantics and wider compatibility:
```sql
WHERE CAST(strftime('%s', timestamp) AS INTEGER) > strftime('%s', 'now') - 86400
CAST((strftime('%s', 'now') - CAST(strftime('%s', timestamp) AS INTEGER)) / 3600 AS INTEGER) AS hours_ago
```

### Braille Encoding

Map `[u32; 24]` → 10-character Braille string using the most recent 10 hours.

**Braille character display width:** All Braille Pattern characters (U+2800–U+28FF) have display width **1** per the `unicode-width` crate (which zero-drift-chat already uses via `UnicodeWidthStr`). The 12-column `right_area` budget is calculated on this basis: 1 left-pad + 10 Braille chars × 1 col each + 1 right-pad = 12 cols. If a target terminal renders these as width 2, the column will overflow — this is an inherent terminal font dependency and out of scope for this spec.

```rust
// In Rust: array[14..] gives indices 14..=23 (10 elements), inclusive on both ends.
// (Rust ranges are exclusive-end: [14..24] == [14..] on a length-24 array)
let slice = &array[14..];   // 10 elements: indices 14, 15, ..., 23

let max_val = *slice.iter().max().unwrap_or(&0);
if max_val == 0 {
    return "⠀".repeat(10);   // guard FIRST, before any division
}
const BRAILLE: [char; 9] = ['⠀','⣀','⣄','⣆','⣇','⣧','⣷','⣾','⣿'];
slice.iter()
    .map(|&v| BRAILLE[((v * 8) / max_val).min(8) as usize])
    .collect()
```

Color: `Green` when `chat.unread_count > 0`, `DarkGray` otherwise.

### AppState Changes

```rust
// In AppState
activity_cache: HashMap<String, [u32; 24]>,  // chat_id → hourly buckets; index 23 = current hour
activity_last_refresh_tick: u64,              // value of tick_count at last full SQL refresh
```

**Initial values in `AppState::new()`:**
```rust
activity_cache: HashMap::new(),
activity_last_refresh_tick: 0,
```

**Refresh schedule** (tick rate ≈ 250 ms → 1200 ticks ≈ 5 min):

| Trigger | Action |
|---------|--------|
| Startup (after chats load) | Full SQL query for all chat IDs |
| `tick_count - activity_last_refresh_tick >= 1200` | Full SQL query for all chat IDs |
| New message received for `chat_id` | `activity_cache.entry(chat_id).or_insert([0u32; 24])[23] += 1` |

**Missing chat IDs:** Use `entry().or_insert([0u32; 24])` on increment so new chats arriving mid-session are initialized to zero before the first full refresh.

**Hour boundary rotation:** ±1 bucket inaccuracy within 5 minutes of an hour boundary is acceptable. The full refresh re-derives all arrays from SQL and corrects any drift. No manual rotation is needed.

### `chat_list.rs` Layout

The graph column is a horizontal split applied to `area` **before** the existing vertical (pinned/unpinned) split:

```
area (full chat panel)
  └─ horizontal split: [area.width.saturating_sub(12), 12]
        ├─ left_area  → existing logic: vertical split into pinned + unpinned sections
        └─ right_area → graph column (borderless; 12 cols = 10 Braille chars + 2 padding cols)
```

**Data plumbing:** The horizontal split and graph column rendering happen inside `render_chat_list` (in `chat_list.rs`). Add `activity_cache: &HashMap<String, [u32; 24]>` as a new parameter to `render_chat_list`. The call site in `render.rs` passes `&app_state.activity_cache`. When `area.width < 100`, the parameter is accepted but unused.

The `right_area` is rendered **without a Block border** so the 10-character Braille string fits exactly within 10 columns with 1 column left-pad and 1 column right-pad.

The graph column renders a `24h` header in the first row (DarkGray, centered), then one Braille string per visible chat row in the same row order as `left_area`.

When `area.width < 100`: skip the horizontal split entirely; `left_area = area`; graph column not rendered.

---

## Error Handling

- **Activity query fails**: log warning, leave `activity_cache` stale; blank graph (`⠀`×10) for any chat not in cache.
- **`sender` empty string**: display as `(unknown)`.
- **`status` unavailable**: treat as `Sent` (single gray `✓`).

---

## Testing

**`activity.rs`:**
- Braille encoding: all-zero input → `⠀`×10
- Braille encoding: single spike at index 23 → rightmost char is `⣿`, rest `⠀`
- Braille encoding: uniform distribution → all chars same non-blank level
- SQL: dynamic placeholder building for 0, 1, and N chat IDs
- `entry().or_insert` initializes missing keys correctly

**`message_view.rs`:**
- Grouping: consecutive same-sender → header only on first; alternating senders → separator each time
- Direction flip (`is_outgoing` changes) → separator inserted
- Right-pad: `saturating_sub` prevents underflow on very long lines
- Last-line pad accounts for status indicator width
- Scroll estimation: `is_outgoing` branches use correct max widths

**Manual:**
- Verify layout at 80, 100, 120, 160 column widths
- Verify graph column aligns with chat rows in both pinned and unpinned sections
- Verify `▌`/`▐` selection markers still appear in `MessageSelect` mode

---

## Files Changed

| File | Change |
|------|--------|
| `src/tui/widgets/message_view.rs` | Replace gutter with colored ┃; two max-widths for scroll estimation |
| `src/tui/widgets/chat_list.rs` | Horizontal split for graph column, compose with existing vertical split |
| `src/tui/app_state.rs` | Add `activity_cache`, `activity_last_refresh_tick` |
| `src/storage/activity.rs` | **New** — 24h SQL query + Braille encoder |
| `src/storage/db.rs` | Expose `Database` ref to activity query |
| `src/app.rs` | Wire activity refresh on startup and every 1200 ticks |
