use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::app_state::{SettingsState, SettingsValue};

pub fn render_settings_overlay(f: &mut Frame, settings: &SettingsState) {
    let area = f.area();

    let popup_width = 46u16.min(area.width);
    let popup_height = (settings.items.len() as u16 + 8).min(area.height);
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup = ratatui::layout::Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Settings ")
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();

    // Blank line
    lines.push(Line::from(""));

    // Dirty indicator
    if settings.dirty {
        lines.push(Line::from(Span::styled(
            "  * unsaved changes",
            Style::default().fg(Color::Yellow),
        )));
    } else {
        lines.push(Line::from(""));
    }

    // Blank line
    lines.push(Line::from(""));

    // Setting items
    for (i, item) in settings.items.iter().enumerate() {
        let selected = i == settings.selected;

        let prefix = if selected { "  > " } else { "    " };
        let label_style = if selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let value_str = match &item.value {
            SettingsValue::Bool(true) => "[x] On ".to_string(),
            SettingsValue::Bool(false) => "[ ] Off".to_string(),
            SettingsValue::Choice(choices, idx) => format!("< {} >", choices[*idx]),
        };

        // Pad label to align values on the right
        let label_width = 24;
        let padded_label = format!("{:<width$}", item.label, width = label_width);

        let line = Line::from(vec![
            Span::styled(prefix, label_style),
            Span::styled(padded_label, label_style),
            Span::styled(value_str, label_style),
        ]);
        lines.push(line);
    }

    // Blank line
    lines.push(Line::from(""));

    // Footer hints
    lines.push(Line::from(Span::styled(
        "  j/k:Navigate  Enter/Space:Toggle",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "  Ctrl+s:Save  Esc:Cancel",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "  (changes take effect on restart)",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}
