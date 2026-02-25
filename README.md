# zero-drift-chat

A unified messaging TUI built in Rust. Aggregates multiple chat platforms into a single terminal interface.

Currently supports **WhatsApp** via [whatsapp-rust](https://github.com/jlucaso1/whatsapp-rust) (WhatsApp Web multi-device protocol).

## Features

- Real WhatsApp messaging — send and receive from your terminal
- QR code authentication rendered directly in the terminal
- Session persistence — scan once, auto-reconnects on restart
- 3-panel TUI: chat list | message view | input bar
- Vim-style keybindings (j/k navigate, i to type, Tab to switch panels)
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

On first launch with WhatsApp enabled, a QR code appears. Scan it with WhatsApp on your phone (Settings > Linked Devices > Link a Device). Subsequent launches auto-reconnect.

### Keybindings

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate chats |
| `Tab` | Switch between chat list and messages |
| `i` / `Enter` | Start typing |
| `Esc` | Back to normal mode |
| `Enter` (in insert mode) | Send message |
| `PgUp` / `PgDn` | Scroll messages |
| `q` | Quit |

## Configuration

Config file: `configs/default.toml`

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

## Building

```bash
git clone https://github.com/mgblackwater/zero-drift-chat
cd zero-drift-chat
cargo build --release
```

Requires nightly Rust (handled automatically via `rust-toolchain.toml`).

## License

MIT
