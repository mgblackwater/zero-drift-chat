# Show Picture Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Press Enter on a selected `[Image]` message to stream-download it to a temp file and open it in the OS default image viewer — zero image bytes held in Rust heap.

**Architecture:** Add `Action::OpenMedia` wired to Enter in `MessageSelect` mode; a new `src/tui/media.rs` module streams the download via `reqwest` + `tokio_util::io::StreamReader` + `tokio::io::copy` straight to disk; `std::process::Command::spawn` opens the file with the OS default viewer detached. Temp files older than 24 h are cleaned up at startup.

**Tech Stack:** Rust, Tokio, reqwest (stream feature), tokio-util (io feature), std::process::Command, xdg-open / open / cmd.

---

## Chunk 1: Dependencies, Data Layer, and media.rs

### Task 1: Enable reqwest stream feature and add tokio-util io feature

**Files:**
- Modify: `Cargo.toml`

**Context:**
- `reqwest 0.12` is already present with `features = ["json"]`. We need to add `"stream"` to get `.bytes_stream()`.
- `tokio-util 0.7` is already present with `features = ["rt"]`. We need to add `"io"` to get `StreamReader`.
- After this change, `cargo build` must compile without errors.

- [ ] **Step 1: Add features to Cargo.toml**

  In `Cargo.toml`, change:
  ```toml
  reqwest = { version = "0.12", features = ["json"] }
  tokio-util = { version = "0.7", features = ["rt"] }
  ```
  to:
  ```toml
  reqwest = { version = "0.12", features = ["json", "stream"] }
  tokio-util = { version = "0.7", features = ["rt", "io"] }
  ```

- [ ] **Step 2: Verify the build compiles**

  Run: `cargo build 2>&1 | tail -5`
  Expected: `Finished` line — no errors.

- [ ] **Step 3: Commit**

  ```bash
  git add Cargo.toml Cargo.lock
  git commit -m "chore: enable reqwest stream + tokio-util io features"
  ```

---

### Task 2: Emit MessageContent::Image from WhatsApp convert

**Files:**
- Modify: `src/providers/whatsapp/convert.rs:248-255`

**Context:**
- `MessageContent::Image { url, caption }` already exists in `src/core/types.rs` but is not used.
- The WhatsApp `image_message` protobuf has a `url` field (CDN URL) and `caption` field.
- Currently the image arm (line 248) maps to `MessageContent::Text("[Image] ...")`. We must change it to `MessageContent::Image { url, caption }` so downstream logic can detect the URL.
- A unit test will verify the conversion returns `MessageContent::Image` when an image URL is present, and falls back to `MessageContent::Text("[Image]")` when the URL is empty/absent.

- [ ] **Step 1: Write the failing test**

  Add at the bottom of `src/providers/whatsapp/convert.rs` inside `#[cfg(test)] mod tests { ... }` (create the block if absent):

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      /// When image_message has a URL, extract_message_content returns Image variant.
      #[test]
      fn test_image_message_with_url_returns_image_content() {
          use whatsapp_rust::waproto::whatsapp as wa;
          let mut msg = wa::Message::default();
          let mut img = wa::ImageMessage::default();
          img.url = Some("https://cdn.example.com/img.jpg".to_string());
          img.caption = Some("A caption".to_string());
          msg.image_message = Some(img);
          let content = extract_message_content(&msg).unwrap();
          match content {
              MessageContent::Image { url, caption } => {
                  assert_eq!(url, "https://cdn.example.com/img.jpg");
                  assert_eq!(caption, Some("A caption".to_string()));
              }
              other => panic!("Expected Image, got {:?}", other),
          }
      }

      /// When image_message has no URL, fall back to Text("[Image]").
      #[test]
      fn test_image_message_no_url_falls_back_to_text() {
          use whatsapp_rust::waproto::whatsapp as wa;
          let mut msg = wa::Message::default();
          let img = wa::ImageMessage::default(); // url = None
          msg.image_message = Some(img);
          let content = extract_message_content(&msg).unwrap();
          match content {
              MessageContent::Text(t) => assert!(t.contains("[Image]")),
              other => panic!("Expected Text fallback, got {:?}", other),
          }
      }
  }
  ```

- [ ] **Step 2: Run tests to confirm they fail**

  Run: `cargo test -p zero-drift-chat test_image_message 2>&1 | tail -20`
  Expected: compile error or FAILED — `MessageContent::Image` is not returned yet.

- [ ] **Step 3: Update extract_message_content to emit MessageContent::Image**

  In `src/providers/whatsapp/convert.rs`, replace lines 248-255:

  ```rust
  if let Some(ref img) = base.image_message {
      return Some(MessageContent::Text(
          match img.caption.as_deref().filter(|s| !s.is_empty()) {
              Some(c) => format!("[Image] {}", c),
              None => "[Image]".to_string(),
          },
      ));
  }
  ```

  with:

  ```rust
  if let Some(ref img) = base.image_message {
      if let Some(ref url) = img.url {
          if !url.is_empty() {
              let caption = img.caption.clone().filter(|s| !s.is_empty());
              return Some(MessageContent::Image {
                  url: url.clone(),
                  caption,
              });
          }
      }
      // Fallback: no URL available — render as text placeholder
      return Some(MessageContent::Text(
          match img.caption.as_deref().filter(|s| !s.is_empty()) {
              Some(c) => format!("[Image] {}", c),
              None => "[Image]".to_string(),
          },
      ));
  }
  ```

- [ ] **Step 4: Run tests to confirm they pass**

  Run: `cargo test -p zero-drift-chat test_image_message 2>&1 | tail -20`
  Expected: `test test_image_message_with_url_returns_image_content ... ok` and `test test_image_message_no_url_falls_back_to_text ... ok`.

- [ ] **Step 5: Run full test suite to confirm no regressions**

  Run: `cargo test 2>&1 | tail -10`
  Expected: all tests pass.

- [ ] **Step 6: Commit**

  ```bash
  git add src/providers/whatsapp/convert.rs
  git commit -m "feat(whatsapp): emit MessageContent::Image with CDN URL"
  ```

---

### Task 3: Emit MessageContent::Image from Telegram convert

**Files:**
- Modify: `src/providers/telegram/convert.rs:49-54`

**Context:**
- Currently the Telegram converter always uses `msg.text()` and falls back to `"[Media]"` — it does not distinguish image vs other media.
- The grammers `Message` API does not expose a direct download URL in a stable public field; the correct approach is to keep the `"[Media]"` / text path for now but distinguish photos specifically: `msg.photo()` returns `Option<grammers_client::types::Photo>` which has no direct CDN URL accessible without a download round-trip.
- **Decision (consistent with spec note "where a media URL is available"):** For Telegram, if `msg.photo().is_some()` and `msg.text().is_empty()`, emit `MessageContent::Text("[Image]")` (identical to the existing `[Media]` fallback but more specific). Full Telegram image opening requires a separate download step using the grammers client — that is **out of scope for v1**. We add the distinction now so the code is consistent and the TODO is visible.
- A unit test verifies the chat-id round-trip (already exists). No new grammers-type unit tests are possible (private constructors).

- [ ] **Step 1: Update grammers_message_to_unified to distinguish photos**

  In `src/providers/telegram/convert.rs`, replace lines 49-54:

  ```rust
  // Text content — media becomes "[Media]" placeholder (v1)
  let text = if msg.text().is_empty() {
      "[Media]".to_string()
  } else {
      msg.text().to_string()
  };
  ```

  with:

  ```rust
  // Text content — distinguish photo vs other media (v1: no URL download yet)
  let content = if !msg.text().is_empty() {
      MessageContent::Text(msg.text().to_string())
  } else if msg.photo().is_some() {
      // TODO(show-picture): Telegram photo download requires grammers client.
      // For now render as [Image] text. A future iteration can add a proper
      // download path via the grammers client handle.
      MessageContent::Text("[Image]".to_string())
  } else {
      MessageContent::Text("[Media]".to_string())
  };
  ```

  Then update the `UnifiedMessage` struct literal to use `content` instead of `MessageContent::Text(text)`:

  Replace:
  ```rust
  content: MessageContent::Text(text),
  ```
  with:
  ```rust
  content,
  ```

  And remove the now-unused `let text = ...` binding (it is replaced by the `content` binding above).

- [ ] **Step 2: Build to confirm it compiles**

  Run: `cargo build 2>&1 | tail -5`
  Expected: `Finished` — no errors.

- [ ] **Step 3: Run full test suite**

  Run: `cargo test 2>&1 | tail -10`
  Expected: all tests pass.

- [ ] **Step 4: Commit**

  ```bash
  git add src/providers/telegram/convert.rs
  git commit -m "feat(telegram): distinguish photo vs other media in converter"
  ```

---

### Task 4: Create src/tui/media.rs with open_image and cleanup_temp_images

**Files:**
- Create: `src/tui/media.rs`
- Modify: `src/tui/mod.rs`

**Context:**
- `open_image(url)` must never hold full image bytes in the Rust heap. The streaming pipeline is: `reqwest` response bytes_stream → `tokio_util::io::StreamReader` → `tokio::io::copy` → `tokio::fs::File`. The read buffer inside `copy` is ~8 KB — that is the maximum in-memory footprint.
- The temp file path is `/tmp/zero-drift-<hash>.jpg`. The hash is computed from the URL using `std::collections::hash_map::DefaultHasher` — no extra allocations.
- The OS opener is selected at compile time via `cfg`:
  - Linux: `xdg-open`
  - macOS: `open`
  - Windows: `cmd /c start "" "<path>"`
- `cleanup_temp_images()` is a synchronous function (uses `std::fs`) that deletes `/tmp/zero-drift-*` files older than 24 hours. It is called once at startup inside `tokio::task::spawn_blocking`.
- Both functions silently ignore all errors (log via `tracing::error!`/`tracing::warn!`).
- `futures = "0.3"` is already in `Cargo.toml` (line 22); `TryStreamExt` from it is used inside `media.rs` — no new crate needed.
- Note: `temp_path_for_url` always appends `.jpg` — a known v1 simplification. Non-JPEG images will still be written correctly (the extension is cosmetic); `cleanup_temp_images` matches on the `zero-drift-` prefix rather than extension so cleanup still works.

- [ ] **Step 1: Write failing unit test for temp_path_for_url**

  Create `src/tui/media.rs` with the test only first:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::temp_path_for_url;

      #[test]
      fn test_temp_path_same_url_gives_same_path() {
          let a = temp_path_for_url("https://cdn.example.com/img.jpg");
          let b = temp_path_for_url("https://cdn.example.com/img.jpg");
          assert_eq!(a, b);
      }

      #[test]
      fn test_temp_path_different_urls_give_different_paths() {
          let a = temp_path_for_url("https://cdn.example.com/img1.jpg");
          let b = temp_path_for_url("https://cdn.example.com/img2.jpg");
          assert_ne!(a, b);
      }

      #[test]
      fn test_temp_path_starts_with_tmp_prefix() {
          let p = temp_path_for_url("https://cdn.example.com/img.jpg");
          let s = p.to_string_lossy();
          assert!(s.contains("zero-drift-"), "path should contain zero-drift-: {}", s);
          assert!(s.ends_with(".jpg"), "path should end with .jpg: {}", s);
      }
  }
  ```

- [ ] **Step 2: Run test to confirm it fails**

  Run: `cargo test -p zero-drift-chat test_temp_path 2>&1 | tail -10`
  Expected: compile error — `temp_path_for_url` does not exist yet.

- [ ] **Step 3: Implement the full media.rs module**

  Replace `src/tui/media.rs` with the full implementation:

  ```rust
  use std::hash::{Hash, Hasher};
  use std::collections::hash_map::DefaultHasher;
  use std::path::PathBuf;
  use std::time::{Duration, SystemTime};

  use futures::TryStreamExt;
  use tokio_util::io::StreamReader;

  /// Generate a deterministic temp file path for a given image URL.
  /// Pattern: /tmp/zero-drift-<u64_hash>.jpg
  pub fn temp_path_for_url(url: &str) -> PathBuf {
      let mut hasher = DefaultHasher::new();
      url.hash(&mut hasher);
      let hash = hasher.finish();
      std::env::temp_dir().join(format!("zero-drift-{}.jpg", hash))
  }

  /// Stream-download `url` to a temp file and open it with the OS default viewer.
  ///
  /// Image bytes flow: network → kernel buffer → disk.
  /// The Rust heap holds only the ~8 KB read buffer inside `tokio::io::copy` —
  /// never the full image.
  pub async fn open_image(url: String) -> anyhow::Result<()> {
      let path = temp_path_for_url(&url);

      // Download only if not already cached
      if !path.exists() {
          let response = reqwest::get(&url).await?;
          let byte_stream = response
              .bytes_stream()
              .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
          let mut reader = StreamReader::new(byte_stream);
          let mut file = tokio::fs::File::create(&path).await?;
          tokio::io::copy(&mut reader, &mut file).await?;
      }

      spawn_os_opener(&path);
      Ok(())
  }

  /// Spawn the OS default image viewer for `path`. Fire-and-forget.
  fn spawn_os_opener(path: &PathBuf) {
      #[cfg(target_os = "linux")]
      let result = std::process::Command::new("xdg-open").arg(path).spawn();

      #[cfg(target_os = "macos")]
      let result = std::process::Command::new("open").arg(path).spawn();

      #[cfg(target_os = "windows")]
      let result = std::process::Command::new("cmd")
          .args(["/c", "start", "", &path.to_string_lossy()])
          .spawn();

      #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
      let result: std::io::Result<std::process::Child> =
          Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "unsupported OS"));

      if let Err(e) = result {
          tracing::error!("Failed to spawn OS opener for {:?}: {}", path, e);
      }
  }

  /// Delete `/tmp/zero-drift-*` files older than 24 hours.
  /// Best-effort: all errors are silently logged.
  pub fn cleanup_temp_images() {
      let cutoff = match SystemTime::now().checked_sub(Duration::from_secs(24 * 3600)) {
          Some(t) => t,
          None => return,
      };

      let tmp = std::env::temp_dir();
      let entries = match std::fs::read_dir(&tmp) {
          Ok(e) => e,
          Err(e) => {
              tracing::warn!("cleanup_temp_images: cannot read {:?}: {}", tmp, e);
              return;
          }
      };

      for entry in entries.flatten() {
          let name = entry.file_name();
          let name_str = name.to_string_lossy();
          if !name_str.starts_with("zero-drift-") {
              continue;
          }
          let path = entry.path();
          let mtime = match entry.metadata().and_then(|m| m.modified()) {
              Ok(t) => t,
              Err(_) => continue,
          };
          if mtime < cutoff {
              if let Err(e) = std::fs::remove_file(&path) {
                  tracing::warn!("cleanup_temp_images: failed to remove {:?}: {}", path, e);
              }
          }
      }
  }

  #[cfg(test)]
  mod tests {
      use super::temp_path_for_url;

      #[test]
      fn test_temp_path_same_url_gives_same_path() {
          let a = temp_path_for_url("https://cdn.example.com/img.jpg");
          let b = temp_path_for_url("https://cdn.example.com/img.jpg");
          assert_eq!(a, b);
      }

      #[test]
      fn test_temp_path_different_urls_give_different_paths() {
          let a = temp_path_for_url("https://cdn.example.com/img1.jpg");
          let b = temp_path_for_url("https://cdn.example.com/img2.jpg");
          assert_ne!(a, b);
      }

      #[test]
      fn test_temp_path_starts_with_tmp_prefix() {
          let p = temp_path_for_url("https://cdn.example.com/img.jpg");
          let s = p.to_string_lossy();
          assert!(s.contains("zero-drift-"), "path should contain zero-drift-: {}", s);
          assert!(s.ends_with(".jpg"), "path should end with .jpg: {}", s);
      }
  }
  ```

- [ ] **Step 4: Declare the module in src/tui/mod.rs**

  Add `pub mod media;` to `src/tui/mod.rs`. The file becomes:

  ```rust
  pub mod app_state;
  pub mod search;
  pub mod event;
  pub mod keybindings;
  pub mod media;
  pub mod osc8;
  pub mod render;
  pub mod time_parse;
  pub mod widgets;
  ```

- [ ] **Step 5: Run tests to confirm temp_path tests pass**

  Run: `cargo test -p zero-drift-chat test_temp_path 2>&1 | tail -10`
  Expected: all 3 tests pass.

- [ ] **Step 6: Run full build and test suite**

  Run: `cargo test 2>&1 | tail -10`
  Expected: all tests pass — no compile errors.

- [ ] **Step 7: Commit**

  ```bash
  git add src/tui/media.rs src/tui/mod.rs
  git commit -m "feat(tui): add media.rs with open_image and cleanup_temp_images"
  ```

---

## Chunk 2: Action Wiring and Startup Cleanup

### Task 5: Add Action::OpenMedia to keybindings

**Files:**
- Modify: `src/tui/keybindings.rs`

**Context:**
- `Action::OpenMedia` must be added to the `Action` enum.
- In `map_message_select_mode`, the current binding is:
  ```rust
  KeyCode::Char('y') | KeyCode::Enter => Action::MessageSelectCopy,
  ```
  Enter should now trigger `Action::OpenMedia`, not `Action::MessageSelectCopy`.
  `y` alone should still trigger `Action::MessageSelectCopy`.
- A unit test verifies:
  1. Enter in MessageSelect mode → `Action::OpenMedia`
  2. `y` in MessageSelect mode → `Action::MessageSelectCopy`
  3. Esc in MessageSelect mode → `Action::MessageSelectExit`

- [ ] **Step 1: Write the failing tests**

  Add inside `src/tui/keybindings.rs` (create `#[cfg(test)] mod tests` if absent):

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

      fn key(code: KeyCode) -> KeyEvent {
          KeyEvent::new(code, KeyModifiers::NONE)
      }

      #[test]
      fn enter_in_message_select_maps_to_open_media() {
          let action = map_key(key(KeyCode::Enter), InputMode::MessageSelect, true);
          assert_eq!(action, Action::OpenMedia);
      }

      #[test]
      fn y_in_message_select_maps_to_copy() {
          let action = map_key(key(KeyCode::Char('y')), InputMode::MessageSelect, true);
          assert_eq!(action, Action::MessageSelectCopy);
      }

      #[test]
      fn esc_in_message_select_maps_to_exit() {
          let action = map_key(key(KeyCode::Esc), InputMode::MessageSelect, true);
          assert_eq!(action, Action::MessageSelectExit);
      }
  }
  ```

- [ ] **Step 2: Run tests to confirm they fail**

  Run: `cargo test -p zero-drift-chat enter_in_message_select 2>&1 | tail -10`
  Expected: compile error — `Action::OpenMedia` does not exist yet.

- [ ] **Step 3: Add Action::OpenMedia to the enum**

  In `src/tui/keybindings.rs`, add `OpenMedia` to the `Action` enum **after `MessageSelectExit` and before `None`** (the `None` arm must remain last):

  ```rust
  MessageSelectExit,  // Esc — exit without copying
  OpenMedia,          // Enter — open image in OS viewer
  ScheduleMessage,    // ... (existing lines continue)
  ```

  The resulting order is: `...MessageSelectExit`, `OpenMedia`, then the existing `ScheduleMessage` / ... / `None` arms unchanged.

- [ ] **Step 4: Update map_message_select_mode**

  Change:
  ```rust
  KeyCode::Char('y') | KeyCode::Enter => Action::MessageSelectCopy,
  ```
  to:
  ```rust
  KeyCode::Char('y') => Action::MessageSelectCopy,
  KeyCode::Enter => Action::OpenMedia,
  ```

- [ ] **Step 5: Run the keybinding tests**

  Run: `cargo test -p zero-drift-chat enter_in_message_select 2>&1 | tail -10`
  Run: `cargo test -p zero-drift-chat y_in_message_select 2>&1 | tail -10`
  Run: `cargo test -p zero-drift-chat esc_in_message_select 2>&1 | tail -10`
  Expected: all 3 pass.

- [ ] **Step 6: Run full test suite**

  Run: `cargo test 2>&1 | tail -10`
  Expected: all tests pass.

- [ ] **Step 7: Commit**

  ```bash
  git add src/tui/keybindings.rs
  git commit -m "feat(keybindings): add Action::OpenMedia, bind Enter in MessageSelect"
  ```

---

### Task 6: Handle Action::OpenMedia in app.rs + spawn startup cleanup

**Files:**
- Modify: `src/app.rs`

**Context:**
- `handle_action` needs a new arm for `Action::OpenMedia`.
- The logic (from spec):
  1. Read `self.state.selected_message_idx`.
  2. If the message at that index has `MessageContent::Image { url, .. }`, clone the URL.
  3. `tokio::spawn` an async block calling `tui::media::open_image(url).await`.
  4. Set `self.state.copy_status = Some("Opening image...".to_string())` for status bar feedback.
  5. Call `self.state.exit_message_select()`.
  6. If the message is NOT an image (e.g., it's text), do nothing except exit message select (do not show any error).
- `startup cleanup`: In `App::run()`, after `self.router.start_all().await?` (around line 152), add:
  ```rust
  tokio::task::spawn_blocking(crate::tui::media::cleanup_temp_images);
  ```
- The `Action::None` arm remains the last arm to avoid compiler warnings.
- A unit test for handle_action is not practical (App is not easily unit-testable — it holds providers, DB, terminal). The integration is verified by running the app and pressing Enter on an `[Image]` message. Manual test steps are described at the end of this task.

- [ ] **Step 1: Add the OpenMedia import — verify media module is accessible**

  In `src/app.rs`, confirm `use crate::tui;` is already present (it is, at line 32). No new import is needed — `crate::tui::media::open_image` is accessible via that path.

- [ ] **Step 2: Add Action::OpenMedia arm in handle_action**

  In `src/app.rs`, inside `handle_action`, add the following arm **before** `Action::None => {}`:

  ```rust
  Action::OpenMedia => {
      if let Some(idx) = self.state.selected_message_idx {
          if let Some(msg) = self.state.messages.get(idx) {
              if let MessageContent::Image { url, .. } = &msg.content {
                  let url = url.clone();
                  tokio::spawn(async move {
                      if let Err(e) = crate::tui::media::open_image(url).await {
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

- [ ] **Step 3: Spawn cleanup task at startup**

  In `src/app.rs`, in `App::run()`, after `self.router.start_all().await?;` (line 152), add:

  ```rust
  tokio::task::spawn_blocking(crate::tui::media::cleanup_temp_images);
  ```

- [ ] **Step 4: Build to confirm it compiles**

  Run: `cargo build 2>&1 | tail -5`
  Expected: `Finished` — no warnings about unused match arms, no errors.

- [ ] **Step 5: Run full test suite**

  Run: `cargo test 2>&1 | tail -10`
  Expected: all tests pass.

- [ ] **Step 6: Manual integration smoke test**

  To verify end-to-end:
  1. Run the app with `cargo run`.
  2. If using the Mock provider: mock messages are text-only, so no `[Image]` messages will appear naturally. You can temporarily patch `src/providers/mock/mod.rs` to emit a `MessageContent::Image { url: "https://...", caption: None }` message, or test with a real WhatsApp account that has received an image.
  3. Navigate to a chat with an `[Image]` message.
  4. Press `v` to enter MessageSelect mode.
  5. Navigate to the `[Image]` message with `j`/`k`.
  6. Press Enter.
  7. Expected: status bar briefly shows `"Opening image..."`, the image opens in the OS viewer, MessageSelect mode exits.
  8. Press `v`, navigate to a text message, press Enter — expected: MessageSelect exits silently (no image opened, no error shown).

- [ ] **Step 7: Commit**

  ```bash
  git add src/app.rs
  git commit -m "feat(app): handle Action::OpenMedia, spawn startup cleanup"
  ```

---

### Task 7: Update WISHLIST.md to mark item #1 complete

**Files:**
- Modify: `docs/WISHLIST.md`

**Context:**
- The wishlist item `[ ] #1 — Show picture` should be checked off and moved to the Completed section.

- [ ] **Step 1: Mark item #1 complete in WISHLIST.md**

  In `docs/WISHLIST.md`, change:
  ```
  - [ ] **Show picture** - Support displaying images/pictures in chat messages.
  ```
  to (remove from the Features list and add to Completed):
  ```
  - [x] **Show picture** - Support displaying images/pictures in chat messages. (2026-03-13)
  ```
  Move that line into the `## Completed` section at the bottom of the file.

- [ ] **Step 2: Commit**

  ```bash
  git add docs/WISHLIST.md
  git commit -m "docs: mark wishlist #1 show-picture as complete"
  ```
