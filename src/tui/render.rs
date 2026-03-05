use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

use super::app_state::{AppState, InputMode};
use super::widgets::{self, chat_list, input_bar, message_view, qr_overlay, settings_overlay, status_bar};

pub fn draw(f: &mut Frame, state: &mut AppState) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(f.area());

    let body = outer[0];
    let status_area = outer[1];

    let body_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Min(1)])
        .split(body);

    let chat_list_area = body_layout[0];
    let message_area = body_layout[1];

    // Input box grows with content: +2 for borders, clamped between 3 (1 line) and 8 (6 lines)
    let input_height = (state.input.lines().len() as u16 + 2).clamp(3, 8);
    let message_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(input_height)])
        .split(message_area);

    let message_view_area = message_layout[0];
    let input_area = message_layout[1];

    // Get current chat name
    let chat_name = state
        .chat_list_state
        .selected()
        .and_then(|i| state.chats.get(i))
        .map(|c| c.name.as_str())
        .unwrap_or("No chat selected");

    chat_list::render_chat_list(
        f,
        chat_list_area,
        &state.chats,
        &mut state.chat_list_state,
        state.active_panel,
        state.input_mode,
    );

    message_view::render_message_view(
        f,
        message_view_area,
        &state.messages,
        chat_name,
        state.scroll_offset,
        state.active_panel,
        state.new_message_count,
    );

    input_bar::render_input_bar(
        f,
        input_area,
        &state.input,
        state.input_mode,
    );

    status_bar::render_status_bar(
        f,
        status_area,
        state.input_mode,
        state.enter_sends,
        state.mock_enabled,
        state.whatsapp_connected,
    );

    // Render QR code overlay on top if present
    if let Some(ref qr) = state.qr_code {
        qr_overlay::render_qr_overlay(f, qr);
    }

    // Render settings overlay on top if open
    if let Some(ref settings) = state.settings_state {
        settings_overlay::render_settings_overlay(f, settings);
    }

    // Render chat context menu popup on top if active
    if state.input_mode == InputMode::ChatMenu {
        if let Some(ref menu_state) = state.chat_menu_state {
            widgets::chat_menu::render_chat_menu(f, chat_list_area, menu_state);
        }
    }

    // Render search overlay on top if active
    if state.input_mode == InputMode::Searching {
        if let Some(ref search) = state.search_state {
            widgets::search_overlay::render_search_overlay(f, chat_list_area, search, &state.chats);
        }
    }
}
