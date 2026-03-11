use ratatui::{
    layout::{Alignment, Rect},
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
    ai_suggestion: Option<&str>,
) {
    let (mode_tag, border_color, title_align) = match mode {
        InputMode::Normal => ("NORMAL", Color::DarkGray, Alignment::Left),
        InputMode::Editing => ("✏  INSERT", Color::Yellow, Alignment::Center),
        InputMode::Settings => ("SETTINGS", Color::Cyan, Alignment::Left),
        InputMode::Renaming => ("RENAME", Color::Magenta, Alignment::Left),
        InputMode::ChatMenu => ("MENU", Color::Yellow, Alignment::Left),
        InputMode::Searching => ("SEARCH", Color::Cyan, Alignment::Left),
        InputMode::MessageSelect => ("SELECT", Color::Blue, Alignment::Left),
    };

    let block = Block::default()
        .title(format!(" {} ", mode_tag))
        .title_alignment(title_align)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if mode == InputMode::Editing || mode == InputMode::Renaming {
        // TextArea widget: handles cursor, horizontal scroll, multi-line display automatically
        f.render_widget(textarea, inner_area);

        // Render AI suggestion hint if present and in editing mode
        if mode == InputMode::Editing {
            if let Some(suggestion) = ai_suggestion {
                if inner_area.height >= 2 {
                    let hint_area = Rect {
                        x: inner_area.x,
                        y: inner_area.y + inner_area.height.saturating_sub(1),
                        width: inner_area.width,
                        height: 1,
                    };
                    let hint_text = format!("  ↳ {}", suggestion);
                    f.render_widget(
                        Paragraph::new(hint_text).style(Style::default().fg(Color::DarkGray)),
                        hint_area,
                    );
                }
            }
        }
    } else {
        // Normal/Settings: render text only, no hardware cursor in the input box
        let text = textarea.lines().join("\n");
        f.render_widget(Paragraph::new(text), inner_area);
    }
}
