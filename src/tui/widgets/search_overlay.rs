use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::core::types::UnifiedChat;
use crate::tui::app_state::SearchState;

pub fn render_search_overlay(
    f: &mut Frame,
    chat_list_area: Rect,
    state: &SearchState,
    chats: &[UnifiedChat],
) {
    let result_count = state.results.len().min(5);
    // 2 borders + 1 query line + (1 divider + N results) if any results
    let inner_height = 1 + if result_count > 0 { 1 + result_count } else { 0 };
    let height = (2 + inner_height as u16).min(chat_list_area.height);

    let popup_area = Rect {
        x: chat_list_area.x,
        y: chat_list_area.y,
        width: chat_list_area.width,
        height,
    };

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Find Chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    // Query line
    let query_area = Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 };
    f.render_widget(
        Paragraph::new(format!("/ {}▌", state.query))
            .style(Style::default().fg(Color::White)),
        query_area,
    );

    if result_count == 0 {
        return;
    }

    // Divider
    let sep_area = Rect { x: inner.x, y: inner.y + 1, width: inner.width, height: 1 };
    f.render_widget(
        Paragraph::new("─".repeat(inner.width as usize))
            .style(Style::default().fg(Color::DarkGray)),
        sep_area,
    );

    // Results list
    let results_area = Rect {
        x: inner.x,
        y: inner.y + 2,
        width: inner.width,
        height: result_count as u16,
    };

    let highlight = Style::default()
        .bg(Color::Blue)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    let items: Vec<ListItem> = state
        .results
        .iter()
        .enumerate()
        .take(result_count)
        .filter_map(|(pos, &idx)| {
            let chat = chats.get(idx)?;
            let name = chat.display_name.as_deref().unwrap_or(&chat.name);
            let selector = if pos == state.selected { "▶ " } else { "  " };
            let tag = format!("[{}] ", chat.platform);
            Some(ListItem::new(Line::from(vec![
                Span::raw(selector),
                Span::styled(tag, Style::default().fg(Color::DarkGray)),
                Span::styled(name.to_string(), Style::default().fg(Color::White)),
            ])))
        })
        .collect();

    let mut list_state = ListState::default();
    let clamped_selected = state.selected.min(items.len().saturating_sub(1));
    list_state.select(Some(clamped_selected));
    f.render_stateful_widget(
        List::new(items).highlight_style(highlight),
        results_area,
        &mut list_state,
    );
}
