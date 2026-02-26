use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

use super::app_state::AppState;
use super::widgets::{chat_list, input_bar, message_view, qr_overlay, settings_overlay, status_bar};

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

    let message_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
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
    );

    message_view::render_message_view(
        f,
        message_view_area,
        &state.messages,
        chat_name,
        state.scroll_offset,
        state.active_panel,
    );

    input_bar::render_input_bar(
        f,
        input_area,
        &state.input_buffer,
        state.cursor_position,
        state.input_mode,
    );

    status_bar::render_status_bar(f, status_area, state.input_mode, state.mock_enabled, state.whatsapp_connected);

    // Render QR code overlay on top if present
    if let Some(ref qr) = state.qr_code {
        qr_overlay::render_qr_overlay(f, qr);
    }

    // Render settings overlay on top if open
    if let Some(ref settings) = state.settings_state {
        settings_overlay::render_settings_overlay(f, settings);
    }
}
