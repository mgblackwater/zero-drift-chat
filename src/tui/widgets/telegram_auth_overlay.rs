use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::app_state::{TelegramAuthStage, TelegramAuthState};

pub fn render_telegram_auth_overlay(f: &mut Frame, state: &TelegramAuthState) {
    let area = f.area();

    let popup_width = 60u16.min(area.width);
    let popup_height = 10u16.min(area.height);
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup = ratatui::layout::Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup);

    let border_color = match state.stage {
        TelegramAuthStage::Phone => Color::Cyan,
        TelegramAuthStage::Otp => Color::Yellow,
        TelegramAuthStage::Password => Color::Magenta,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(state.stage.title())
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(""));

    // Prompt
    lines.push(Line::from(Span::styled(
        format!("  {}", state.stage.prompt()),
        Style::default().fg(Color::White),
    )));

    lines.push(Line::from(""));

    // Error hint (shown on retry)
    if let Some(ref hint) = state.error_hint {
        lines.push(Line::from(Span::styled(
            format!("  ! {}", hint),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    } else {
        lines.push(Line::from(""));
    }

    lines.push(Line::from(""));

    // Input field (mask password input)
    let display_input = if state.stage.is_password() {
        "*".repeat(state.input.len())
    } else {
        state.input.clone()
    };
    lines.push(Line::from(vec![
        Span::styled("  > ", Style::default().fg(border_color)),
        Span::styled(
            display_input,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("_", Style::default().fg(Color::DarkGray)), // cursor
    ]));

    lines.push(Line::from(""));

    // Footer hints
    lines.push(Line::from(Span::styled(
        "  Enter: Submit   Esc: Cancel",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}
