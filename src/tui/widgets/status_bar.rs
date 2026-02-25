use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app_state::InputMode;

pub fn render_status_bar(f: &mut Frame, area: Rect, mode: InputMode, whatsapp_connected: bool) {
    let hints = match mode {
        InputMode::Normal => "q:Quit | Tab:Switch | j/k:Navigate | i:Type | r:Rename | s:Settings",
        InputMode::Editing => "Esc:Normal | Enter:Send | Type your message",
        InputMode::Settings => "j/k:Navigate | Enter/Space:Toggle | Ctrl+s:Save | Esc:Cancel",
        InputMode::Renaming => "Enter:Confirm | Esc:Cancel | Type new name",
    };

    let mut spans = vec![
        Span::styled(" ● ", Style::default().fg(Color::Green)),
        Span::styled("Mock", Style::default().fg(Color::DarkGray)),
    ];

    // WhatsApp status indicator
    let wa_color = if whatsapp_connected {
        Color::Green
    } else {
        Color::Red
    };
    spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(" ● ", Style::default().fg(wa_color)));
    spans.push(Span::styled("WA", Style::default().fg(Color::DarkGray)));

    spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(hints, Style::default().fg(Color::DarkGray)));

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).style(Style::default().bg(Color::Black));
    f.render_widget(paragraph, area);
}
