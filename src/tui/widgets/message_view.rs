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

/// Word-wrap `text` so each output line is at most `max_w` columns wide.
/// Breaks on word boundaries; splits mid-word only when a single word
/// exceeds `max_w`. Always returns at least one element (empty string for
/// empty input so callers can rely on a non-empty vec).
fn wrap_to_width(text: &str, max_w: usize) -> Vec<String> {
    if max_w == 0 {
        return vec![text.to_string()];
    }
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut lines: Vec<String> = Vec::new();
    for original_line in text.split('\n') {
        if original_line.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in original_line.split_whitespace() {
            // Handle words longer than max_w: chop them
            let mut word_remaining = word;
            while !word_remaining.is_empty() {
                let space_needed = if current.is_empty() { 0 } else { 1 };
                let available = max_w.saturating_sub(current.len() + space_needed);
                if available == 0 {
                    // Flush current line
                    lines.push(current.clone());
                    current.clear();
                    continue;
                }
                if word_remaining.len() <= available {
                    if !current.is_empty() {
                        current.push(' ');
                    }
                    current.push_str(word_remaining);
                    word_remaining = "";
                } else {
                    // Check if word fits on a fresh line
                    if current.is_empty() {
                        // Force chop
                        let (chunk, rest) = word_remaining.split_at(available.min(word_remaining.len()));
                        current.push_str(chunk);
                        lines.push(current.clone());
                        current.clear();
                        word_remaining = rest;
                    } else {
                        // Flush and retry on fresh line
                        lines.push(current.clone());
                        current.clear();
                    }
                }
            }
        }
        if !current.is_empty() || lines.last().map(|l: &String| !l.is_empty()).unwrap_or(true) {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

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
            .trim_end_matches(['.', ',', ')', ']', '>', '!', '?'])
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

#[allow(clippy::too_many_arguments)]
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

        let content_width = area.width.saturating_sub(2) as usize;
        let bubble_max_w = (content_width * 70 / 100).max(20);

        for original_line in msg.content.as_text().split('\n') {
        for text_line in wrap_to_width(original_line, bubble_max_w) {
        let text_line = &text_line;
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
        } // end wrap_to_width loop
        } // end original_line loop
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
    let bubble_max_w = (content_width * 70 / 100).max(20);
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
                // Use bubble_max_w since message lines are pre-wrapped to that width.
                line_width.div_ceil(bubble_max_w)
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
    use super::{split_line_with_urls, wrap_to_width};

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

    // wrap_to_width tests
    #[test]
    fn wrap_empty_string() {
        assert_eq!(wrap_to_width("", 10), vec![""]);
    }

    #[test]
    fn wrap_short_text_no_wrap() {
        assert_eq!(wrap_to_width("hello", 10), vec!["hello"]);
    }

    #[test]
    fn wrap_exact_fit() {
        assert_eq!(wrap_to_width("hello", 5), vec!["hello"]);
    }

    #[test]
    fn wrap_two_words_on_separate_lines() {
        assert_eq!(wrap_to_width("hello world", 7), vec!["hello", "world"]);
    }

    #[test]
    fn wrap_long_word_forced_break() {
        // "abcdefghij" is 10 chars, max_w=6: "abcdef" + "ghij"
        assert_eq!(wrap_to_width("abcdefghij", 6), vec!["abcdef", "ghij"]);
    }

    #[test]
    fn wrap_multiline_input() {
        let result = wrap_to_width("line one\nline two", 20);
        assert_eq!(result, vec!["line one", "line two"]);
    }

    #[test]
    fn wrap_multiple_words_wrap_correctly() {
        let result = wrap_to_width("one two three four", 9);
        // "one two" = 7, "three" = 5 (fits alone), "four" = 4
        assert_eq!(result, vec!["one two", "three", "four"]);
    }
}
