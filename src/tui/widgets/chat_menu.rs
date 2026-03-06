use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
    Frame,
};

use crate::tui::app_state::ChatMenuState;

pub fn render_chat_menu(f: &mut Frame, parent_area: Rect, state: &ChatMenuState) {
    // Calculate popup size centered over parent_area
    let popup_width = 28u16.min(parent_area.width.saturating_sub(2));
    let popup_height = (state.items.len() as u16 + 4).min(parent_area.height.saturating_sub(2));
    let x = parent_area.x + (parent_area.width.saturating_sub(popup_width)) / 2;
    let y = parent_area.y + (parent_area.height.saturating_sub(popup_height)) / 2;
    let area = Rect::new(x, y, popup_width, popup_height);

    // Clear background before rendering popup
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = state
        .items
        .iter()
        .map(|item| {
            ListItem::new(Line::from(Span::raw(format!(
                " {}",
                item.label(state.is_pinned, state.is_muted)
            ))))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(" {} ", state.chat_name))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut list_state);
}
