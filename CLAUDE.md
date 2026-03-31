# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`zero-drift-chat` is a privacy-first unified messaging TUI (Terminal User Interface) built in Rust. It aggregates WhatsApp, Telegram, and other chat platforms into a single terminal interface with SQLite-backed local persistence.

## Commands

```bash
# Build
cargo build              # Debug build
cargo build --release    # Release build

# Run tests
cargo test               # All tests
cargo test <test_name>   # Single test by name
cargo test -- --nocapture  # With visible stdout

# Code quality (required before commit)
cargo fmt                # Format code
cargo fmt --check        # Check formatting (CI enforces this)
cargo clippy -- -D warnings  # Lint with strict warnings
```

Rust toolchain: **nightly** (set via `rust-toolchain.toml`).

## Architecture

### Top-Level Structure

```
src/
├── main.rs          # CLI parsing, logging, DB init, app startup
├── app.rs           # Main event loop (~1400 lines), TUI rendering, event dispatch
├── core/            # Shared types, provider trait, router
├── providers/       # Platform implementations (WhatsApp, Telegram, Mock)
├── storage/         # SQLite DB, chat/message CRUD, activity analytics
├── tui/             # UI state machine, widgets, keybindings
├── config/          # AppConfig loaded from TOML
└── ai/              # Async AI autocomplete worker (OpenAI/Anthropic/Gemini backends)
```

### Core Abstractions

**`MessagingProvider` trait** (`src/core/provider.rs`) — async trait that all platform integrations implement. Methods: connect, disconnect, send_message, download_media, get_chats, mark_read, etc.

**`MessageRouter`** (`src/core/router.rs`) — manages provider lifecycle and multiplexes `ProviderEvent` streams from all active providers into a single channel consumed by `app.rs`.

**Unified types** (`src/core/types.rs`) — `UnifiedMessage`, `UnifiedChat`, `Platform`, `MessageContent`, `MessageStatus` abstract over platform differences throughout the codebase.

### Data Flow

```
Keyboard/Mouse input
    → crossterm event stream
    → app.rs event loop
    → AppState mutation + re-render

Platform events (new message, status update, typing, auth)
    → Provider implementation
    → ProviderEvent channel (mpsc unbounded)
    → MessageRouter.poll_events()
    → app.rs match arm
    → Database write + AppState update + re-render
```

### TUI State Machine (`src/tui/app_state.rs`)

Central `AppState` struct holds all UI state. Input mode is an enum: `Normal`, `Editing`, `Settings`, `Searching`, `MessageSelect`, `SchedulePrompt`, `TelegramAuth`. Modal overlays (settings, search, QR, schedule) are driven by these modes.

### Storage (`src/storage/`)

SQLite via `rusqlite` with a migration system in `db.rs`. Key tables: `chats`, `messages`, `sessions`, `scheduled_messages`, `lid_pn_map`. Each concern has its own file (`chats.rs`, `messages.rs`, `schedule.rs`, etc.). Activity analytics (`activity.rs`) queries 24-hour hourly buckets and encodes them as Braille sparklines via `encode_braille()`.

### Provider Notes

- **WhatsApp** (`src/providers/whatsapp/`): Uses `whatsapp-rust` crate (custom git dep). Handles QR auth, session persistence, E2EE media decryption. LID↔PN JID mapping is stored in `lid_pn_map` table.
- **Telegram** (`src/providers/telegram/`): Uses `grammers-client` from Codeberg. Interactive auth flow (phone → OTP → optional 2FA password).
- **Mock** (`src/providers/mock/`): Used in tests.

### Widget Layout (`src/tui/widgets/`)

Ratatui widgets are split by concern: `chat_list.rs`, `message_view.rs`, `input_bar.rs`, `status_bar.rs`, plus overlay widgets (`settings_overlay.rs`, `qr_overlay.rs`, `telegram_auth_overlay.rs`, `schedule_overlay.rs`, `search_overlay.rs`, `chat_menu.rs`). The `render.rs` module orchestrates layout and calls into widgets.

## Key Conventions

- All provider methods are `async` and use `#[async_trait]`.
- Use `anyhow::Result` for error propagation throughout.
- Structured logging via `tracing` macros (`debug!`, `info!`, `warn!`, `error!`); logs go to file (not stdout) to avoid interfering with TUI.
- Config is loaded from a TOML file (default path via `dirs` crate); `AppConfig` in `src/config/settings.rs`.
- Activity cache on `AppState` is refreshed on startup, each tick, and on new messages.
