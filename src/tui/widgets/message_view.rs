use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::core::types::UnifiedMessage;
use crate::tui::app_state::ActivePanel;

pub fn render_message_view(
    f: &mut Frame,
    area: Rect,
    messages: &[UnifiedMessage],
    chat_name: &str,
    scroll_offset: u16,
    active_panel: ActivePanel,
) {
    let border_color = if active_panel == ActivePanel::MessageView {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    if messages.is_empty() {
        let block = Block::default()
            .title(format!(" {} ", chat_name))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));
        let p = Paragraph::new("No messages yet. Press 'i' to start typing.")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(p, area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    for msg in messages {
        let time = msg.timestamp.format("%H:%M").to_string();

        let (sender_color, msg_color) = if msg.is_outgoing {
            (Color::Green, Color::White)
        } else {
            (Color::Cyan, Color::Gray)
        };

        let header = Line::from(vec![
            Span::styled(
                format!("{} ", msg.sender),
                Style::default().fg(sender_color),
            ),
            Span::styled(time, Style::default().fg(Color::DarkGray)),
        ]);

        let content = Line::from(Span::styled(
            msg.content.as_text().to_string(),
            Style::default().fg(msg_color),
        ));

        lines.push(header);
        lines.push(content);
        lines.push(Line::from("")); // spacing
    }

    // Auto-scroll: calculate how many lines we can see and scroll to bottom
    let visible_height = area.height.saturating_sub(2) as usize; // subtract borders
    let total_lines = lines.len();
    let auto_scroll = if total_lines > visible_height {
        (total_lines - visible_height) as u16
    } else {
        0
    };

    // Apply manual scroll offset (scroll_offset moves UP from auto-scroll position)
    let effective_scroll = auto_scroll.saturating_sub(scroll_offset);

    let block = Block::default()
        .title(format!(" {} ", chat_name))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((effective_scroll, 0));

    f.render_widget(paragraph, area);
}
