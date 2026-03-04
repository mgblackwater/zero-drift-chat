# Clickable Links in Message View — Design

## Goal

URLs in chat messages should be visually distinct and openable via Ctrl+Click in Windows Terminal (WSL). Drag-to-select any text should auto-copy to clipboard via Windows Terminal's native selection.

## Root Cause

`EnableMouseCapture` was added as scaffolding boilerplate in Phase 1 and was never used — no mouse events are routed in the app. Its only effect is disabling the terminal's native text selection, breaking drag-to-copy for no benefit.

## Design

### Part 1: Remove EnableMouseCapture

Remove `EnableMouseCapture` and `DisableMouseCapture` from all three call sites in `src/app.rs` (startup, panic hook, cleanup). This restores Windows Terminal's native drag-to-select, which automatically copies selected text to the clipboard.

### Part 2: OSC 8 Hyperlinks in Message View

Detect URLs in message content using a simple regex and render them as [OSC 8 hyperlinks](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda). Ratatui supports OSC 8 via `ratatui::text::Span` with a `link` modifier. Windows Terminal renders these as underlined, clickable links — Ctrl+Click opens the URL in the default browser.

URL regex: `https?://[^\s<>"{}|\\^`\[\]]+`

Each message line is split into alternating plain-text and URL spans. URLs are styled with underline and a distinct color (e.g. `Color::LightBlue`) plus the OSC 8 hyperlink attribute.

### Data Flow

```
msg.content.as_text()
  → split by \n into lines
    → each line: regex scan for URLs
      → plain segments → Span::raw
      → URL segments   → Span::styled(...).underlined() + OSC 8 link
  → pushed to lines: Vec<Line>
  → rendered by Paragraph as before
```

No changes to `AppState`, storage, or keybindings.

## Constraints

- ratatui 0.29 supports `Stylize::underlined()` and OSC 8 via `Span::styled(...).underline_color(...)` — check exact API
- URL regex kept simple; no dependency on `regex` crate if `once_cell` + stdlib suffices
- No mouse event handling added (EnableMouseCapture removed, not replaced)
