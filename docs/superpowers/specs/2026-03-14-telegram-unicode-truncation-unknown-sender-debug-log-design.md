# Design: Telegram Unicode Truncation Fix, Unknown Sender Fix, Debug Logging

**Date:** 2026-03-14  
**Branch:** fix/telegram-unicode-truncation-debug-log  
**Status:** Approved

---

## Problem Summary

Three related issues affecting Telegram message display:

1. **Message truncation** — AI-generated bot messages containing emoji and rich Unicode are visually truncated in the TUI. Root cause: `wrap_to_width` and the scroll height estimator operate on **bytes** (`str::len`, `str::split_at`) instead of **display columns**. Each emoji is 4 bytes but 2 display columns; multi-byte splits cause content to be dropped or panic-split mid-codepoint.

2. **Unknown sender still shown** — The `fix/telegram-unknown-username` fix populates `ChatNameCache` and threads it into `get_messages`, but the **live update loop** inside `connect_and_run` calls `grammers_message_to_unified(&msg, &chat_id, None)` — `fallback_name` is always `None` there, so channel messages received in real-time still show "Unknown".

3. **No observability** — There is no way to compare the raw message text as received from Telegram against what the TUI renders, making it hard to diagnose future truncation regressions.

---

## Architecture

No new modules. All changes are contained within:

| File | Change |
|---|---|
| `src/tui/widgets/message_view.rs` | Fix `wrap_to_width` and scroll estimator to use `unicode_width` |
| `src/providers/telegram/mod.rs` | Forward `chat_name_cache` into `connect_and_run`; pass as `fallback_name` in update loop |
| `src/main.rs` (or init site) | Add optional file tracing subscriber controlled by `ZERO_DRIFT_DEBUG=1` |
| `Cargo.toml` | Add `unicode-width` dependency (if not already transitive) |

---

## Component Design

### 1. Unicode-aware `wrap_to_width`

**Current (broken):**
```rust
let available = max_w.saturating_sub(current.len() + space_needed);
// ...
let (chunk, rest) = word_remaining.split_at(available.min(word_remaining.len()));
```

**Fixed approach:**
- Measure widths using `unicode_width::UnicodeWidthStr::width(s)` for string width and `unicode_width::UnicodeWidthChar::width(c).unwrap_or(0)` per character.
- Replace the `split_at(n)` byte-chop with a char-boundary-safe split: accumulate chars until the column budget is exhausted, then `split_at` on the resulting byte offset.
- `current.len()` → `UnicodeWidthStr::width(current.as_str())`

**Invariant:** The output `Vec<String>` is still valid UTF-8; each string's display width ≤ `max_w` columns.

### 2. Unicode-aware scroll height estimator

**Current (broken):**
```rust
let line_width: usize = line.spans.iter().map(|s| s.content.len()).sum();
```

**Fixed:**
```rust
let line_width: usize = line.spans.iter()
    .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
    .sum();
```

This ensures the auto-scroll calculation matches what ratatui actually renders (ratatui uses `unicode_width` internally).

### 3. Forward `chat_name_cache` into `connect_and_run`

`connect_and_run` currently receives `peer_cache` but not `chat_name_cache`. It must:
1. Accept `chat_name_cache: ChatNameCache` as a new parameter.
2. Populate it during the dialog iteration loop (same as `get_chats` does).
3. In the update loop, resolve `fallback_name` from the cache before calling `grammers_message_to_unified`.

```rust
// In the update loop:
let fallback = chat_name_cache.get(&chat_id);
if let Some(unified) = grammers_message_to_unified(&msg, &chat_id, fallback.as_deref()) {
```

`start()` already clones `peer_cache` before the spawn — add the same clone for `chat_name_cache`.

### 4. Debug file logging

Controlled by env var `ZERO_DRIFT_DEBUG=1`.

In the app's initialization (wherever `tracing_subscriber` is currently configured):

```rust
if std::env::var("ZERO_DRIFT_DEBUG").is_ok() {
    // Add a rolling file appender writing to zero-drift-debug.log
    let file_appender = tracing_appender::rolling::never(".", "zero-drift-debug.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    // combine with existing subscriber or use a layered approach
}
```

Store `_guard` in a top-level variable to keep the background writer alive for the process lifetime.

At message receipt in `grammers_message_to_unified` (or immediately after in `get_messages` / the update loop), emit:

```rust
tracing::debug!(
    chat_id = %chat_id,
    msg_id = %msg.id(),
    raw_text = %msg.text(),
    "telegram raw message received"
);
```

This logs the **pre-wrap** full text. The TUI renders the post-wrap version. Diffing the log against the TUI output immediately shows whether truncation is in the data layer or the render layer.

**Dependencies to add to `Cargo.toml`:**
```toml
unicode-width = "0.2"
tracing-appender = "0.2"
```

(`unicode-width` may already be in the lock file transitively via ratatui; either way, declaring it explicitly is correct.)

---

## Data Flow

```
Telegram MTProto
      │
      ▼
grammers_message_to_unified()
      │
      ├─── tracing::debug!(raw_text) ──► zero-drift-debug.log  (when ZERO_DRIFT_DEBUG=1)
      │
      ▼
UnifiedMessage { content: MessageContent::Text(full_text) }
      │
      ▼
render_message_view()
      │
      ├── wrap_to_width(text, bubble_max_w)   ← Unicode-column-aware
      │        └── UnicodeWidthStr::width()
      │
      └── scroll estimator                    ← Unicode-column-aware
               └── UnicodeWidthStr::width()
```

---

## Error Handling

- `tracing_appender` file creation failure: non-fatal; log a warning to stderr and continue without file logging.
- `unicode-width` returns 0 for control characters and unprintable codepoints — this is correct behavior (they contribute no visual columns).
- Characters with `None` width (e.g., combining marks) are treated as 0-width — consistent with terminal behavior.

---

## Testing

### Unit tests — `wrap_to_width`
Add tests in `message_view.rs`:
- Single emoji "🎉" at width 2 → fits on one line of max_w=2
- String of 5 emoji at width 2 each → each on its own line when max_w=2
- Mixed ASCII + emoji wrapping

### Manual verification
1. Set `ZERO_DRIFT_DEBUG=1`, open a Telegram bot chat with emoji-heavy messages.
2. Compare `zero-drift-debug.log` (raw) vs TUI display.
3. Verify sender name is no longer "Unknown" for channel messages.

---

## Out of Scope

- Downloading and rendering Telegram bot rich-text formatting (bold, italic, inline code) — the grammers `text()` method returns plain text stripped of formatting entities. This is a separate feature.
- Double-width CJK character handling — `unicode_width` handles this correctly by default; no extra work needed.
