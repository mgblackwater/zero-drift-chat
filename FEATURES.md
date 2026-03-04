# Features

## Messaging
- Real WhatsApp messaging — send and receive from your terminal
- QR code authentication rendered directly in the terminal
- Session persistence — scan once, auto-reconnects on restart
- WhatsApp history sync — auto-populates chat list with group names and contacts
- SQLite message storage with full chat history
- Mock provider for testing without a WhatsApp account

## Interface
- 3-panel TUI: chat list | message view | input bar
- Multi-line input — `Enter` inserts a newline, `Shift+Enter` / `Alt+Enter` sends
- Cursor navigation in input — arrow keys, `Home`, `End`, `Ctrl+U` to clear
- Vim-style keybindings (`j`/`k` navigate, `i` to type, `Tab` to switch panels)
- In-app settings overlay — toggle providers and log level without editing files
- Chat rename — press `r` to set custom display names (persisted across restarts)
- Version displayed in status bar
