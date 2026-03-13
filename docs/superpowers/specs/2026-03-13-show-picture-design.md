# Design: Show Picture — Open Image with Default OS App

**Date:** 2026-03-13  
**Status:** Approved  
**Wishlist item:** #1 — Show picture

---

## Goal

When a message contains an image (`[Image]` placeholder), the user can press Enter while the message is selected to download and open it in the OS default image viewer — without loading image bytes into the Rust process heap.

---

## Constraints

- Zero image bytes held in the Rust process memory at any time.
- Works on Linux, macOS, and Windows (the three supported platforms).
- No new UI overlays or modes required.
- Fits the existing `MessageSelect` mode UX pattern.

---

## Design

### 1. Data Layer — Use `MessageContent::Image` Properly

**Files:** `src/core/types.rs`, `src/providers/whatsapp/convert.rs`, `src/providers/telegram/convert.rs`

`MessageContent::Image { url, caption }` already exists in the type system but is unused — both providers currently downcast images to `MessageContent::Text("[Image] ...")`.

**Change:** Update `extract_message_content` in `whatsapp/convert.rs` to emit `MessageContent::Image { url, caption }` using the CDN URL already present in the WhatsApp protobuf (`img.url`). Apply the same change to Telegram's `convert.rs` where a media URL is available.

`MessageContent::as_text()` already returns `"[Image]"` / caption as a fallback — no change needed there. The rendered `[Image]` label in the TUI remains unchanged.

### 2. New Module — `src/tui/media.rs`

Single public async function:

```rust
pub async fn open_image(url: String) -> anyhow::Result<()>
```

**Steps:**

1. **Generate temp path** — `/tmp/zero-drift-<hash>.jpg` where `<hash>` is a short deterministic hash of the URL (using `std::collections::hash_map::DefaultHasher` — no allocation for image bytes).

2. **Stream download** — `reqwest::get(&url).await` returns a streaming response. Use `.bytes_stream()` piped through `tokio_util::io::StreamReader` into `tokio::io::copy` writing to a `tokio::fs::File`. Image bytes flow: network → kernel buffer → disk. The Rust heap holds only a small read buffer (~8 KB), never the full image.

3. **Spawn OS opener** (detached, fire-and-forget):
   - Linux: `xdg-open <path>`
   - macOS: `open <path>`
   - Windows: `cmd /c start "" "<path>"`
   - Selected at compile time via `#[cfg(target_os = "...")]`.
   - `std::process::Command::new(opener).arg(&path).spawn()` — not awaited.

4. Return `Ok(())`.

**Error handling:** If download or open fails, log the error via `tracing::error!`. No panic. Status bar will not show an error (keep it simple for v1).

### 3. Temp File Cleanup — `cleanup_temp_images()`

A best-effort function called once at app startup (non-blocking, spawned as a `tokio::task`):

```rust
fn cleanup_temp_images() {
    // Glob /tmp/zero-drift-*.{jpg,png,gif,webp}
    // Delete files older than 24 hours
    // Silently ignore all errors
}
```

Uses `std::fs::read_dir("/tmp")` + metadata mtime comparison. No extra dependencies.

### 4. Action Wiring

**`src/tui/keybindings.rs`**

Add `Action::OpenMedia` to the `Action` enum.

Map `Enter` in `InputMode::MessageSelect` → `Action::OpenMedia`.

> Currently `Enter` has no binding in `MessageSelect` mode. `y` is `MessageSelectCopy`, `Esc`/`q` is `MessageSelectExit`.

**`src/app.rs` — `handle_action`**

```rust
Action::OpenMedia => {
    if let Some(idx) = self.state.selected_message_idx {
        if let Some(msg) = self.state.messages.get(idx) {
            if let MessageContent::Image { url, .. } = &msg.content {
                let url = url.clone();
                tokio::spawn(async move {
                    if let Err(e) = media::open_image(url).await {
                        tracing::error!("Failed to open image: {}", e);
                    }
                });
                self.state.copy_status = Some("Opening image...".to_string());
            }
        }
    }
    self.state.exit_message_select();
}
```

### 5. Status Feedback

Reuses the existing `copy_status` flash mechanism in the status bar. `"Opening image..."` appears for ~1 tick (~250 ms) then clears automatically — consistent with the `"Copied!"` feedback pattern.

---

## Dependencies

- `reqwest` — already in `Cargo.toml` (used by AI providers). Add `stream` feature if not already enabled.
- `tokio-util` — add to `Cargo.toml` with `io` feature, for `StreamReader`.
- No other new dependencies.

---

## Out of Scope

- Video/audio/document open (can follow the same pattern later).
- Progress indicator while downloading.
- Caching already-downloaded images (temp file reuse by hash covers the common case).
- Error feedback in the status bar for failed downloads (tracing log only for v1).

---

## File Touch List

| File | Change |
|---|---|
| `src/core/types.rs` | No change needed |
| `src/providers/whatsapp/convert.rs` | Emit `MessageContent::Image { url, caption }` |
| `src/providers/telegram/convert.rs` | Emit `MessageContent::Image { url, caption }` where available |
| `src/tui/media.rs` | New — `open_image()` + `cleanup_temp_images()` |
| `src/tui/mod.rs` | Declare `pub mod media` |
| `src/tui/keybindings.rs` | Add `Action::OpenMedia`, map Enter in MessageSelect |
| `src/app.rs` | Handle `Action::OpenMedia`, spawn cleanup task at startup |
| `Cargo.toml` | Add `tokio-util` with `io` feature; ensure `reqwest/stream` |
