# zero-drift-chat

A unified messaging TUI built in Rust. Aggregates multiple chat platforms into a single terminal interface.

Currently supports **WhatsApp** via [whatsapp-rust](https://github.com/jlucaso1/whatsapp-rust) (WhatsApp Web multi-device protocol).

## Features

- Real WhatsApp messaging — send and receive from your terminal
- QR code authentication rendered directly in the terminal
- Session persistence — scan once, auto-reconnects on restart
- 3-panel TUI: chat list | message view | input bar
- Vim-style keybindings (j/k navigate, i to type, Tab to switch panels)
- In-app settings overlay — toggle providers and log level without editing files
- Chat rename — press `r` to set custom display names (persisted across restarts)
- WhatsApp history sync — auto-populates chat list with group names and contacts
- SQLite message storage with full chat history
- Mock provider for testing without a WhatsApp account

## Install

### From release (no Rust needed)

**macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/mgblackwater/zero-drift-chat/master/install.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/mgblackwater/zero-drift-chat/master/install.ps1 | iex
```

### From source

Requires [Rust nightly](https://rustup.rs/):

```bash
cargo install --git https://github.com/mgblackwater/zero-drift-chat
```

## Usage

```bash
zero-drift-chat
```

On first launch with WhatsApp enabled, a QR code appears. Scan it with WhatsApp on your phone (Settings > Linked Devices > Link a Device). Make sure the terminal window is large enough for the QR code to render fully. Subsequent launches auto-reconnect.

### Keybindings

**Normal mode:**

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate chats |
| `Tab` | Switch between chat list and messages |
| `i` / `Enter` | Start typing |
| `r` | Rename selected chat |
| `s` | Open settings |
| `PgUp` / `PgDn` | Scroll messages |
| `q` | Quit |

**Insert mode:**

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Esc` | Back to normal mode |

**Settings overlay:**

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate settings |
| `Enter` / `Space` | Toggle option |
| `Ctrl+s` | Save to config file |
| `Esc` | Cancel and close |

Settings changes take effect on restart.

## Configuration

Config file: `configs/default.toml` (auto-created with defaults if missing)

You can edit it manually or use the in-app settings overlay (`s` key).

```toml
[general]
log_level = "info"

[tui]
tick_rate_ms = 250
render_rate_ms = 33
chat_list_width_percent = 30

[mock_provider]
enabled = false

[whatsapp]
enabled = true
```

### Data locations

| File | Path |
|------|------|
| Database | `~/.zero-drift-chat/zero-drift.db` |
| WhatsApp session | `~/.zero-drift-chat/whatsapp-session.db` |
| Logs | `~/.zero-drift-chat/zero-drift.log` |

### Troubleshooting

- **QR code won't scan:** Make the terminal window larger. The QR must render fully without clipping.
- **WhatsApp pairing stuck:** Delete `~/.zero-drift-chat/whatsapp-session.db*` and restart to re-pair.
- **Chats show phone numbers:** Custom names can be set with `r`. Group names populate automatically via history sync on first connect after pairing.

## Building

```bash
git clone https://github.com/mgblackwater/zero-drift-chat
cd zero-drift-chat
cargo build --release
```

Requires nightly Rust (handled automatically via `rust-toolchain.toml`).

## License

MIT
