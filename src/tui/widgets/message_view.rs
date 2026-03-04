use chrono::Local;
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
        let time = msg.timestamp.with_timezone(&Local).format("%H:%M").to_string();

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

        lines.push(header);
        for text_line in msg.content.as_text().split('\n') {
            lines.push(Line::from(Span::styled(
                text_line.to_string(),
                Style::default().fg(msg_color),
            )));
        }
        lines.push(Line::from("")); // spacing
    }

    // Padding so the last message is never clipped by word-wrap miscalculation
    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // Auto-scroll: estimate total visual lines accounting for word-wrap
    let visible_height = area.height.saturating_sub(2) as usize; // subtract borders
    let content_width = area.width.saturating_sub(2) as usize; // subtract borders
    let total_lines: usize = lines
        .iter()
        .map(|line| {
            if content_width == 0 {
                return 1;
            }
            let line_width: usize = line.spans.iter().map(|s| s.content.len()).sum();
            if line_width == 0 {
                1
            } else {
                // +1 accounts for ratatui word-wrap sometimes needing an extra line
                1 + line_width / content_width
            }
        })
        .sum();
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
