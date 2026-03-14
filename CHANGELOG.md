# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `--help` flag shows app description via clap (feat/T004-module-a)
- `SECURITY.md` vulnerability reporting policy (feat/T004-module-b)

## [0.3.4] - 2026-03-14

### Fixed

- Long message bubbles now capped at 70% panel width
- Telegram sender names resolved correctly (no longer shows "Unknown")
- Windows build failure (bundled SQLite, LNK1181 error resolved)
- Compiler warnings suppressed (dead_code, unused variable)

## v0.3.3 — 2026-03-13

### View images in your terminal chat

WhatsApp image messages can now be opened directly from the TUI.

- In **Message Select mode** (`v`), navigate to any `[Image]` message and press `Enter` (or `v` again)
- The image is downloaded, decrypted end-to-end using the WhatsApp media key, and opened in your OS default image viewer
- A **"Opening image..."** flash appears in the status bar while the download is in progress
- Images without E2EE metadata (e.g. some history-sync thumbnails) fall back to a direct CDN fetch
- Temporary files are written to `/tmp/` and cleaned up automatically on the next startup
- A brief **"Failed to open image: …"** error is shown in the status bar if the download or viewer launch fails

#### New internal pieces
- `MediaDecryptParams` struct carries the `media_key`, `direct_path`, `file_sha256`, `file_enc_sha256`, `file_length`, and `mime_type` fields extracted from the WhatsApp image proto
- `Provider::download_media()` trait method — `WhatsAppProvider` implements it via `download_from_params` (AES-256-CBC + HMAC, handled by the `whatsapp_rust` library)
- `open_image_from_bytes()` in `tui/media.rs` writes bytes to a temp file with the correct extension (from MIME type or URL) and hands it to the OS viewer

## v0.3.2 — 2026-03-11

### Copy messages to clipboard

- Press `y` in Normal mode to instantly copy the **last message** in the current chat to the clipboard
- Press `v` in Normal mode to enter **Message Select mode** — a vim-style selection overlay
  - `j` / `↓` and `k` / `↑` navigate between messages; the selected message is highlighted with a cyan `▌` gutter and blue background
  - `y` or `Enter` copies the selected message text and exits Select mode
  - `Esc` / `q` cancels without copying
- Clipboard is written via the **OSC 52** terminal escape sequence — works in any modern terminal (kitty, iTerm2, tmux with `set-clipboard on`, etc.) without external tools
- A brief **"Copied!"** flash appears in the status bar to confirm the copy

## v0.3.1 — 2026-03-05

- Newsletter chats (`@newsletter` JIDs) now show a `[NL]` tag and are excluded from the unread count header
- New messages are highlighted in yellow with a full-width `─── N new ───` separator when navigating to a chat
- Reading messages on another device (phone/web) now clears the unread count and `─── N new ───` separator via `ReadSelf` receipts
- Selected chat row now uses blue background with white text for clearer focus indication
- Press `/` to open a fuzzy chat search popup — type to filter, top 5 results shown, `j`/`k` to navigate, `Enter` to jump to chat and enter insert mode, `Esc` to cancel
- INSERT mode is now clearly indicated across the UI: mode pill badge in the status bar (`✏ INSERT` in yellow), centered label on the input box border, and the chat list header shows the current chat name

## v0.3.0 — 2026-03-04

### Address Book Separation
- Moved display names to a dedicated `addressbook.db` SQLite database
- Chat renames now persist independently from the main `zero-drift.db`
- On startup, display names from the address book are applied to loaded chats

### `--reset` CLI Flag
- Added `--reset` flag to delete `zero-drift.db` and `whatsapp-session.db` for a clean re-pair
- Address book (`addressbook.db`) is preserved across resets
- Also cleans up SQLite WAL/SHM journal files
- Gracefully warns instead of crashing if files are locked by another process

### WhatsApp Improvements
- **History sync via JoinedGroup events**: replaced `HistorySync` handler with per-conversation `JoinedGroup` processing, emitting individual messages for full chat history on re-pair
- **LID-to-PN JID normalization**: added `JidCache` that maps WhatsApp's internal LID JIDs to phone number JIDs, preventing duplicate chats for the same contact
- **WebMessageInfo conversion**: new `web_msg_to_unified()` converts history sync messages with proper sender names, timestamps, and delivery status
- **SyncCompleted event**: new provider event triggers a UI refresh after WhatsApp offline sync finishes

### UI Fixes
- **Timestamps now display in local time** instead of UTC
- **Fixed message view scroll cutoff**: auto-scroll calculation now accounts for word-wrapped lines and includes bottom padding so the last message is always fully visible

## v0.2.0 — 2026-02-28

- Phase 3: Settings overlay, chat rename, WhatsApp improvements
- Mock provider toggle, active chats pushed to top
- Chat history loading on startup

## v0.1.0 — 2026-02-20

- Initial release: mock provider, TUI, SQLite storage, WhatsApp integration
