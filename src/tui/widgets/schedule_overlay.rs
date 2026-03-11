use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{block::Title, Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::tui::app_state::ScheduleListState;
use crate::tui::time_parse::format_local_time;

pub fn render_schedule_list_overlay(
    f: &mut Frame,
    state: &ScheduleListState,
    chat_names: &std::collections::HashMap<String, String>,
) {
    let area = f.area();
    let count = state.messages.len();

    // Each entry = 3 lines (name, preview, time) + 1 blank between
    let content_height = if count == 0 {
        1
    } else {
        count * 3 + (count - 1)
    };
    let inner_height = content_height + 1; // +1 for footer hints
    let height = (inner_height as u16 + 2).min(area.height.saturating_sub(4)); // +2 for borders
    let width = (area.width * 60 / 100)
        .max(40)
        .min(area.width.saturating_sub(4));

    let popup = Rect {
        x: (area.width.saturating_sub(width)) / 2,
        y: (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    f.render_widget(Clear, popup);

    let title = Title::from(Line::from(vec![Span::styled(
        " Scheduled Messages ",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if count == 0 {
        let empty =
            Paragraph::new("  No scheduled messages").style(Style::default().fg(Color::DarkGray));
        f.render_widget(empty, inner);
        return;
    }

    let highlight = Style::default()
        .bg(Color::Cyan)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    let items: Vec<ListItem> = state
        .messages
        .iter()
        .enumerate()
        .map(|(i, msg)| {
            let selector = if i == state.selected { "▶ " } else { "  " };
            let chat_name = chat_names
                .get(&msg.chat_id)
                .cloned()
                .unwrap_or_else(|| msg.chat_id.clone());
            let platform_tag = format!("[{}] ", msg.platform);
            let preview = msg.content.as_text();
            let preview = if preview.len() > 40 {
                &preview[..40]
            } else {
                preview
            };
            let time_str = format!("→ {}", format_local_time(&msg.send_at));

            ListItem::new(vec![
                Line::from(vec![
                    Span::raw(selector.to_string()),
                    Span::styled(platform_tag, Style::default().fg(Color::DarkGray)),
                    Span::styled(chat_name, Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled(format!("\"{}\"", preview), Style::default().fg(Color::Gray)),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled(time_str, Style::default().fg(Color::Cyan)),
                ]),
            ])
        })
        .collect();

    // Render the list with the footer area reserved
    let list_height = inner.height.saturating_sub(1);
    let list_area = Rect {
        height: list_height,
        ..inner
    };
    let footer_area = Rect {
        y: inner.y + list_height,
        height: 1,
        ..inner
    };

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));
    f.render_stateful_widget(
        List::new(items).highlight_style(highlight),
        list_area,
        &mut list_state,
    );

    // Footer hints
    let hints = Paragraph::new(Line::from(vec![
        Span::styled("j/k", Style::default().fg(Color::Yellow)),
        Span::styled(":Navigate  ", Style::default().fg(Color::DarkGray)),
        Span::styled("d", Style::default().fg(Color::Yellow)),
        Span::styled(":Cancel  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::styled(":Close", Style::default().fg(Color::DarkGray)),
    ]));
    f.render_widget(hints, footer_area);
}
