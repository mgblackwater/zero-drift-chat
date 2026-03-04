use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use tui_textarea::TextArea;

use crate::tui::app_state::InputMode;

pub fn render_input_bar(
    f: &mut Frame,
    area: Rect,
    textarea: &TextArea<'static>,
    mode: InputMode,
) {
    let (mode_tag, border_color) = match mode {
        InputMode::Normal => ("NORMAL", Color::DarkGray),
        InputMode::Editing => ("INSERT", Color::Yellow),
        InputMode::Settings => ("SETTINGS", Color::Cyan),
        InputMode::Renaming => ("RENAME", Color::Magenta),
    };

    let block = Block::default()
        .title(format!(" [{}] ", mode_tag))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if mode == InputMode::Editing || mode == InputMode::Renaming {
        // TextArea widget: handles cursor, horizontal scroll, multi-line display automatically
        f.render_widget(textarea.widget(), inner_area);
    } else {
        // Normal/Settings: render text only, no hardware cursor in the input box
        let text = textarea.lines().join("\n");
        f.render_widget(Paragraph::new(text), inner_area);
    }
}
