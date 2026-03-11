use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

use crate::core::types::UnifiedChat;
use crate::tui::app_state::{ActivePanel, InputMode};

fn make_item(chat: &UnifiedChat, is_selected: bool) -> ListItem<'static> {
    let tag = format!("[{}]", chat.platform);
    let unread = if chat.unread_count > 0 {
        format!(" ({})", chat.unread_count)
    } else {
        String::new()
    };
    let name = chat
        .display_name
        .as_deref()
        .unwrap_or(&chat.name)
        .to_string();
    let selector = if is_selected { "▶ " } else { "  " };
    let pin_tag = if chat.is_pinned { "* " } else { "  " };

    // Muted chats render dimmed
    let (name_color, unread_color) = if chat.is_muted {
        (Color::DarkGray, Color::Gray)
    } else {
        (Color::White, Color::Yellow)
    };

    let mut spans = vec![
        Span::raw(selector),
        Span::styled(pin_tag.to_string(), Style::default().fg(Color::Yellow)),
    ];
    if chat.is_newsletter {
        spans.push(Span::styled("[NL]", Style::default().fg(Color::Cyan)));
    } else if chat.is_group {
        spans.push(Span::styled("[GP]", Style::default().fg(Color::Magenta)));
    } else {
        spans.push(Span::styled(tag, Style::default().fg(Color::DarkGray)));
    }
    spans.push(Span::raw(" "));
    spans.push(Span::styled(name, Style::default().fg(name_color)));
    spans.push(Span::styled(unread, Style::default().fg(unread_color)));
    ListItem::new(Line::from(spans))
}

pub fn render_chat_list(
    f: &mut Frame,
    area: Rect,
    chats: &[UnifiedChat],
    list_state: &mut ListState,
    active_panel: ActivePanel,
    input_mode: InputMode,
) {
    let border_color = if active_panel == ActivePanel::ChatList {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let (title, title_alignment) = if input_mode == InputMode::Editing {
        let chat_name = list_state
            .selected()
            .and_then(|i| chats.get(i))
            .map(|c| c.display_name.as_deref().unwrap_or(&c.name))
            .unwrap_or("—");
        (format!(" ✏  {} ", chat_name), Alignment::Center)
    } else {
        let total_unread: u32 = chats
            .iter()
            .filter(|c| !c.is_newsletter && !c.is_muted)
            .map(|c| c.unread_count)
            .sum();
        let t = if total_unread > 0 {
            format!(" Chats ({}) ", total_unread)
        } else {
            " Chats ".to_string()
        };
        (t, Alignment::Left)
    };

    let block = Block::default()
        .title(title)
        .title_alignment(title_alignment)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let selected = list_state.selected().unwrap_or(0);
    let pinned_count = chats.iter().filter(|c| c.is_pinned).count();

    let highlight = Style::default()
        .bg(Color::Blue)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    if pinned_count == 0 {
        // No pinned chats — plain scrollable list
        let items: Vec<ListItem> = chats
            .iter()
            .enumerate()
            .map(|(i, chat)| make_item(chat, i == selected))
            .collect();
        // No highlight_symbol — selector is embedded in item content
        let list = List::new(items).highlight_style(highlight);
        f.render_stateful_widget(list, inner, list_state);
        return;
    }

    // Split inner area: fixed pinned section on top, scrollable unpinned below
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(pinned_count as u16), Constraint::Min(0)])
        .split(inner);

    // --- Pinned section (always visible, no scroll) ---
    let pinned_items: Vec<ListItem> = chats
        .iter()
        .filter(|c| c.is_pinned)
        .enumerate()
        .map(|(i, chat)| make_item(chat, selected < pinned_count && i == selected))
        .collect();
    let mut pinned_state = ListState::default();
    if selected < pinned_count {
        pinned_state.select(Some(selected));
    }
    let pinned_list = List::new(pinned_items).highlight_style(highlight);
    f.render_stateful_widget(pinned_list, sections[0], &mut pinned_state);

    // --- Unpinned section (scrollable) ---
    let unpinned_items: Vec<ListItem> = chats
        .iter()
        .filter(|c| !c.is_pinned)
        .enumerate()
        .map(|(i, chat)| {
            make_item(
                chat,
                selected >= pinned_count && i == selected - pinned_count,
            )
        })
        .collect();
    let mut unpinned_state = ListState::default();
    if selected >= pinned_count {
        unpinned_state.select(Some(selected - pinned_count));
    }
    let unpinned_list = List::new(unpinned_items).highlight_style(highlight);
    f.render_stateful_widget(unpinned_list, sections[1], &mut unpinned_state);
}
