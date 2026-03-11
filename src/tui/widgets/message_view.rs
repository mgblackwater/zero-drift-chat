use chrono::Local;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::core::types::UnifiedMessage;
use crate::tui::app_state::ActivePanel;

/// Split a line of text into alternating (segment, is_url) pairs.
/// URLs are contiguous non-whitespace text starting with "http://" or "https://".
fn split_line_with_urls(line: &str) -> Vec<(&str, bool)> {
    let mut result = Vec::new();
    let mut remaining = line;
    while !remaining.is_empty() {
        let http_pos = remaining.find("http://");
        let https_pos = remaining.find("https://");
        let url_start = match (http_pos, https_pos) {
            (None, None) => {
                result.push((remaining, false));
                break;
            }
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (Some(a), Some(b)) => a.min(b),
        };
        if url_start > 0 {
            result.push((&remaining[..url_start], false));
        }
        let url_text = &remaining[url_start..];
        let url_end = url_text
            .find(|c: char| c.is_whitespace())
            .unwrap_or(url_text.len());
        // Strip trailing sentence punctuation that is not part of the URL
        let raw_url = &url_text[..url_end];
        let stripped_len = raw_url
            .trim_end_matches(|c| matches!(c, '.' | ',' | ')' | ']' | '>' | '!' | '?'))
            .len();
        let (url_part, punct_part) = raw_url.split_at(stripped_len);
        if !url_part.is_empty() {
            result.push((url_part, true));
        }
        if !punct_part.is_empty() {
            result.push((punct_part, false));
        }
        remaining = &url_text[url_end..];
    }
    result
}

pub fn render_message_view(
    f: &mut Frame,
    area: Rect,
    messages: &[UnifiedMessage],
    chat_name: &str,
    scroll_offset: u16,
    active_panel: ActivePanel,
    new_message_count: usize,
    selected_message_idx: Option<usize>,
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

    // Find the index of the first "new" message by counting backwards
    // through incoming messages until we reach new_message_count.
    let new_start_idx = if new_message_count > 0 {
        let mut counted = 0;
        let mut idx = None;
        for (i, msg) in messages.iter().enumerate().rev() {
            if !msg.is_outgoing {
                counted += 1;
                if counted == new_message_count {
                    idx = Some(i);
                    break;
                }
            }
        }
        idx
    } else {
        None
    };

    let mut lines: Vec<Line> = Vec::new();

    for (i, msg) in messages.iter().enumerate() {
        // Insert a full-width "─── N new ───" separator before the first new message
        if Some(i) == new_start_idx {
            let content_width = area.width.saturating_sub(2) as usize;
            let label = format!(" {} new ", new_message_count);
            let dashes = content_width.saturating_sub(label.len());
            let left = dashes / 2;
            let right = dashes - left;
            let separator = format!("{}{}{}", "─".repeat(left), label, "─".repeat(right));
            lines.push(Line::styled(separator, Style::default().fg(Color::Yellow)));
        }

        let time = msg
            .timestamp
            .with_timezone(&Local)
            .format("%H:%M")
            .to_string();
        let is_new = new_start_idx.map(|s| i >= s).unwrap_or(false) && !msg.is_outgoing;
        let is_selected = selected_message_idx == Some(i);

        let (sender_color, msg_color) = if msg.is_outgoing {
            (Color::Green, Color::White)
        } else if is_new {
            (Color::Yellow, Color::White)
        } else {
            (Color::Cyan, Color::Gray)
        };

        // Selection highlight style applied to every span in the selected message
        let select_bg = if is_selected {
            Style::default().bg(Color::Blue)
        } else {
            Style::default()
        };

        // Left gutter marker: only shown when this message is selected
        let gutter = "▌ ";
        let gutter_style = Style::default().fg(Color::Cyan).bg(Color::Blue);

        let header = if msg.is_outgoing {
            // Right-aligned: "10:02  You" pushed to the right edge
            let mut line = Line::from(vec![
                Span::styled(time.clone(), select_bg.fg(Color::DarkGray)),
                Span::styled(format!("  {}", msg.sender), select_bg.fg(sender_color)),
            ])
            .alignment(Alignment::Right);
            if is_selected {
                // For outgoing selected, append a trailing marker on the right
                line = Line::from(vec![
                    Span::styled(time, select_bg.fg(Color::DarkGray)),
                    Span::styled(format!("  {}", msg.sender), select_bg.fg(sender_color)),
                    Span::styled(" ▐", Style::default().fg(Color::Cyan).bg(Color::Blue)),
                ])
                .alignment(Alignment::Right);
            }
            line
        } else if is_selected {
            // Left-aligned selected: cyan gutter + highlighted sender + time
            Line::from(vec![
                Span::styled(gutter, gutter_style),
                Span::styled(format!("{} ", msg.sender), select_bg.fg(sender_color)),
                Span::styled(time, select_bg.fg(Color::DarkGray)),
            ])
        } else {
            // Left-aligned normal: no gutter
            Line::from(vec![
                Span::styled(
                    format!("{} ", msg.sender),
                    Style::default().fg(sender_color),
                ),
                Span::styled(time, Style::default().fg(Color::DarkGray)),
            ])
        };

        lines.push(header);
        let url_style = Style::default()
            .fg(Color::LightBlue)
            .add_modifier(Modifier::UNDERLINED);

        for text_line in msg.content.as_text().split('\n') {
            let line = if !text_line.contains("http://") && !text_line.contains("https://") {
                if is_selected && !msg.is_outgoing {
                    Line::from(vec![
                        Span::styled(gutter, gutter_style),
                        Span::styled(text_line.to_string(), select_bg.fg(msg_color)),
                    ])
                } else if is_selected {
                    Line::from(Span::styled(text_line.to_string(), select_bg.fg(msg_color)))
                } else {
                    Line::from(Span::styled(
                        text_line.to_string(),
                        Style::default().fg(msg_color),
                    ))
                }
            } else {
                let spans: Vec<Span> =
                    if is_selected && !msg.is_outgoing {
                        let mut s = vec![Span::styled(gutter, gutter_style)];
                        s.extend(split_line_with_urls(text_line).into_iter().map(
                            |(seg, is_url)| {
                                if is_url {
                                    Span::styled(
                                        seg.to_string(),
                                        select_bg
                                            .fg(Color::LightBlue)
                                            .add_modifier(Modifier::UNDERLINED),
                                    )
                                } else {
                                    Span::styled(seg.to_string(), select_bg.fg(msg_color))
                                }
                            },
                        ));
                        s
                    } else {
                        split_line_with_urls(text_line)
                            .into_iter()
                            .map(|(seg, is_url)| {
                                if is_url {
                                    Span::styled(
                                        seg.to_string(),
                                        if is_selected {
                                            select_bg
                                                .fg(Color::LightBlue)
                                                .add_modifier(Modifier::UNDERLINED)
                                        } else {
                                            url_style
                                        },
                                    )
                                } else {
                                    Span::styled(
                                        seg.to_string(),
                                        if is_selected {
                                            select_bg.fg(msg_color)
                                        } else {
                                            Style::default().fg(msg_color)
                                        },
                                    )
                                }
                            })
                            .collect()
                    };
                Line::from(spans)
            };
            if msg.is_outgoing {
                lines.push(line.alignment(Alignment::Right));
            } else {
                lines.push(line);
            }
        }
        lines.push(Line::from("")); // spacing
    }

    // Padding so the last message is never clipped by word-wrap miscalculation
    lines.push(Line::from(""));
    lines.push(Line::from(""));
    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // Auto-scroll: estimate total visual lines accounting for word-wrap.
    // We use ceiling division (no +1 per line) to avoid over-estimating, and
    // rely on the 4 blank padding lines above to absorb any remaining error.
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
                // ceiling division: how many rows does this line occupy after wrapping?
                (line_width + content_width - 1) / content_width
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

#[cfg(test)]
mod tests {
    use super::split_line_with_urls;

    #[test]
    fn plain_text_no_url() {
        assert_eq!(
            split_line_with_urls("hello world"),
            vec![("hello world", false)]
        );
    }

    #[test]
    fn empty_string() {
        assert!(split_line_with_urls("").is_empty());
    }

    #[test]
    fn single_url() {
        assert_eq!(
            split_line_with_urls("https://example.com"),
            vec![("https://example.com", true)]
        );
    }

    #[test]
    fn url_with_trailing_period() {
        assert_eq!(
            split_line_with_urls("See https://example.com."),
            vec![("See ", false), ("https://example.com", true), (".", false)]
        );
    }

    #[test]
    fn url_in_parens() {
        assert_eq!(
            split_line_with_urls("(https://example.com)"),
            vec![("(", false), ("https://example.com", true), (")", false)]
        );
    }

    #[test]
    fn url_with_trailing_comma() {
        assert_eq!(
            split_line_with_urls("check https://example.com, and more"),
            vec![
                ("check ", false),
                ("https://example.com", true),
                (",", false),
                (" and more", false)
            ]
        );
    }

    #[test]
    fn two_urls_in_one_line() {
        assert_eq!(
            split_line_with_urls("a https://foo.com b https://bar.com c"),
            vec![
                ("a ", false),
                ("https://foo.com", true),
                (" b ", false),
                ("https://bar.com", true),
                (" c", false),
            ]
        );
    }

    #[test]
    fn http_and_https_both_detected() {
        assert_eq!(
            split_line_with_urls("http://a.com"),
            vec![("http://a.com", true)]
        );
        assert_eq!(
            split_line_with_urls("https://a.com"),
            vec![("https://a.com", true)]
        );
    }
}
