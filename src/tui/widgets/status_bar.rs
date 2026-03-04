use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app_state::InputMode;

pub fn render_status_bar(
    f: &mut Frame,
    area: Rect,
    mode: InputMode,
    enter_sends: bool,
    mock_enabled: bool,
    whatsapp_connected: bool,
) {
    let hints = match mode {
        InputMode::Normal => "q:Quit | i:Insert | s:Settings | r:Rename | x:Menu | Tab:Switch",
        InputMode::Editing => {
            if enter_sends {
                "Esc:Normal | Enter:Send | Shift+Enter:Newline | Ctrl+S:Send | Ctrl+U:Clear"
            } else {
                "Esc:Normal | Enter:Newline | Shift+Enter/Ctrl+S:Send | Ctrl+U:Clear"
            }
        }
        InputMode::Settings => "j/k:Navigate | Enter/Space:Toggle | Ctrl+s:Save | Esc:Cancel",
        InputMode::Renaming => "Enter:Confirm | Esc:Cancel | Type new name",
        InputMode::ChatMenu => "j/k:Navigate | p/Enter:Confirm | Esc:Close",
    };

    let mut spans = Vec::new();

    // Version
    spans.push(Span::styled(
        format!(" v{} ", env!("CARGO_PKG_VERSION")),
        Style::default().fg(Color::DarkGray),
    ));
    spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));

    if mock_enabled {
        spans.push(Span::styled(" ● ", Style::default().fg(Color::Green)));
        spans.push(Span::styled("Mock", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
    }

    // WhatsApp status indicator
    let wa_color = if whatsapp_connected {
        Color::Green
    } else {
        Color::Red
    };
    spans.push(Span::styled(" ● ", Style::default().fg(wa_color)));
    spans.push(Span::styled("WA", Style::default().fg(Color::DarkGray)));

    spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(hints, Style::default().fg(Color::DarkGray)));

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).style(Style::default().bg(Color::Black));
    f.render_widget(paragraph, area);
}
