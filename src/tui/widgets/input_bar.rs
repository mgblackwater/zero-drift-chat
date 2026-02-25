use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::app_state::InputMode;

pub fn render_input_bar(
    f: &mut Frame,
    area: Rect,
    input: &str,
    cursor_position: usize,
    mode: InputMode,
) {
    let (mode_tag, border_color) = match mode {
        InputMode::Normal => ("NORMAL", Color::DarkGray),
        InputMode::Editing => ("INSERT", Color::Yellow),
        InputMode::Settings => ("SETTINGS", Color::Cyan),
        InputMode::Renaming => ("RENAME", Color::Magenta),
    };

    let line = Line::from(vec![
        Span::styled(
            format!("[{}] ", mode_tag),
            Style::default().fg(border_color),
        ),
        Span::raw(input),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let paragraph = Paragraph::new(line).block(block);
    f.render_widget(paragraph, area);

    // Show cursor when editing
    if mode == InputMode::Editing || mode == InputMode::Renaming {
        let prefix_len = mode_tag.len() + 3; // "[MODE] "
        let byte_pos = cursor_position;
        let char_offset = input[..byte_pos].chars().count();
        f.set_cursor_position((
            area.x + 1 + prefix_len as u16 + char_offset as u16,
            area.y + 1,
        ));
    }
}
