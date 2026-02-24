use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app_state::InputMode;

pub fn render_status_bar(f: &mut Frame, area: Rect, mode: InputMode) {
    let hints = match mode {
        InputMode::Normal => "q:Quit | Tab:Switch | j/k:Navigate | i:Type | PgUp/PgDn:Scroll",
        InputMode::Editing => "Esc:Normal | Enter:Send | Type your message",
    };

    let line = Line::from(vec![
        Span::styled(" ● ", Style::default().fg(Color::Green)),
        Span::styled("Mock", Style::default().fg(Color::DarkGray)),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        Span::styled(hints, Style::default().fg(Color::DarkGray)),
    ]);

    let paragraph = Paragraph::new(line).style(Style::default().bg(Color::Black));
    f.render_widget(paragraph, area);
}
