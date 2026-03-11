# Changelog

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
