use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

use crate::core::types::UnifiedChat;
use crate::tui::app_state::ActivePanel;

pub fn render_chat_list(
    f: &mut Frame,
    area: Rect,
    chats: &[UnifiedChat],
    list_state: &mut ListState,
    active_panel: ActivePanel,
) {
    let items: Vec<ListItem> = chats
        .iter()
        .map(|chat| {
            let tag = format!("[{}]", chat.platform);
            let unread = if chat.unread_count > 0 {
                format!(" ({})", chat.unread_count)
            } else {
                String::new()
            };

            let name = chat.display_name.as_deref().unwrap_or(&chat.name);
            let line = Line::from(vec![
                Span::styled(tag, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(name, Style::default().fg(Color::White)),
                Span::styled(unread, Style::default().fg(Color::Yellow)),
            ]);

            ListItem::new(line)
        })
        .collect();

    let border_color = if active_panel == ActivePanel::ChatList {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Chats ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, list_state);
}
