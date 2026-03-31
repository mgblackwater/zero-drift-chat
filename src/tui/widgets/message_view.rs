use chrono::Local;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::core::types::{MessageStatus, UnifiedMessage};
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
        let mut current_w: usize = 0;
        for word in original_line.split_whitespace() {
            let word_w = UnicodeWidthStr::width(word);
            let space_needed = if current.is_empty() { 0 } else { 1 };
            if current_w + space_needed + word_w <= max_w {
                // Word fits on current line
                if !current.is_empty() {
                    current.push(' ');
                    current_w += 1;
                }
                current.push_str(word);
                current_w += word_w;
            } else if word_w > max_w {
                // Word wider than max_w: flush current, then chop word char by char
                if !current.is_empty() {
                    lines.push(current.clone());
                    current.clear();
                    current_w = 0;
                }
                let mut chunk = String::new();
                let mut chunk_w: usize = 0;
                for ch in word.chars() {
                    let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
                    if chunk_w + ch_w > max_w {
                        lines.push(chunk.clone());
                        chunk.clear();
                        chunk_w = 0;
                    }
                    chunk.push(ch);
                    chunk_w += ch_w;
                }
                if !chunk.is_empty() {
                    current = chunk;
                    current_w = chunk_w;
                }
            } else {
                // Word doesn't fit on current line; flush and start fresh
                if !current.is_empty() {
                    lines.push(current.clone());
                }
                current = word.to_string();
                current_w = word_w;
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

/// Build content spans for a single pre-wrapped line with optional URL styling.
/// `is_selected`: adds blue background when true.
/// `text_color`: base message text color.
fn build_content_spans(line: &str, is_selected: bool, text_color: Color) -> Vec<Span<'static>> {
    let bg = if is_selected {
        Color::Blue
    } else {
        Color::Black
    };
    let url_style = Style::default()
        .fg(Color::LightBlue)
        .add_modifier(Modifier::UNDERLINED)
        .bg(bg);

    if !line.contains("http://") && !line.contains("https://") {
        return vec![Span::styled(
            line.to_string(),
            Style::default().fg(text_color).bg(bg),
        )];
    }

    split_line_with_urls(line)
        .into_iter()
        .map(|(seg, is_url)| {
            if is_url {
                Span::styled(seg.to_string(), url_style)
            } else {
                Span::styled(seg.to_string(), Style::default().fg(text_color).bg(bg))
            }
        })
        .collect()
}

/// Returns the display column width of the delivery status indicator.
fn display_width_of_status(status: MessageStatus) -> usize {
    match status {
        MessageStatus::Sending => 3,   // "···"  (3 × U+00B7)
        MessageStatus::Sent => 1,      // "✓"
        MessageStatus::Delivered => 2, // "✓✓"
        MessageStatus::Read => 2,      // "✓✓"
        MessageStatus::Failed => 1,    // "✗"
    }
}

/// Returns the styled delivery-status span.
fn status_span(status: MessageStatus) -> Span<'static> {
    let (text, color) = match status {
        MessageStatus::Sending => ("···", Color::DarkGray),
        MessageStatus::Sent => ("✓", Color::DarkGray),
        MessageStatus::Delivered => ("✓✓", Color::DarkGray),
        MessageStatus::Read => ("✓✓", Color::Green),
        MessageStatus::Failed => ("✗", Color::Red),
    };
    Span::styled(text, Style::default().fg(color))
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
        // Insert "─── N new ───" separator before first new message
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
        let is_selected = selected_message_idx == Some(i);

        // Group boundary: direction flip, different sender, or >5min gap
        let prev = if i > 0 { messages.get(i - 1) } else { None };
        let is_group_start = match prev {
            None => true,
            Some(p) => {
                p.is_outgoing != msg.is_outgoing
                    || p.sender != msg.sender
                    || (msg.timestamp - p.timestamp).num_seconds().abs() > 300
            }
        };

        // Blank separator between groups (not before the very first message)
        if is_group_start && i > 0 {
            lines.push(Line::from(""));
        }

        let area_w = area.width.saturating_sub(2) as usize;
        let sender_display = if msg.sender.is_empty() {
            "(unknown)".to_string()
        } else {
            msg.sender.clone()
        };

        if msg.is_outgoing {
            // ── Outgoing: right-aligned, cyan ┃ ──────────────────────────────
            if is_group_start {
                let header = format!("{} {}", sender_display, time);
                let pad = area_w.saturating_sub(header.len());
                let header_line = if is_selected {
                    Line::from(vec![
                        Span::raw(" ".repeat(pad)),
                        Span::styled(
                            sender_display.clone(),
                            Style::default().bg(Color::Blue).fg(Color::Cyan),
                        ),
                        Span::styled(
                            format!(" {}", time),
                            Style::default().bg(Color::Blue).fg(Color::DarkGray),
                        ),
                        Span::styled(" ▐", Style::default().fg(Color::Cyan).bg(Color::Blue)),
                    ])
                } else {
                    Line::from(vec![
                        Span::raw(" ".repeat(pad)),
                        Span::styled(sender_display.clone(), Style::default().fg(Color::Cyan)),
                        Span::styled(format!(" {}", time), Style::default().fg(Color::DarkGray)),
                    ])
                };
                lines.push(header_line);
            }

            let max_self_w = (area_w * 2 / 3).max(20);
            let content_text = msg.content.as_text().to_string();
            let mut all_wrapped: Vec<String> = Vec::new();
            for original_line in content_text.split('\n') {
                all_wrapped.extend(wrap_to_width(original_line, max_self_w));
            }
            let total = all_wrapped.len();
            for (li, text_line) in all_wrapped.iter().enumerate() {
                let is_last = li == total - 1;
                let line_w = UnicodeWidthStr::width(text_line.as_str());
                let mut spans: Vec<Span> =
                    build_content_spans(text_line, is_selected, Color::White);
                if is_last {
                    let status_w = display_width_of_status(msg.status);
                    let pad = area_w.saturating_sub(line_w + 2 + 1 + status_w);
                    let mut row: Vec<Span> = vec![Span::raw(" ".repeat(pad))];
                    row.append(&mut spans);
                    row.push(Span::styled(
                        " ┃",
                        Style::default().fg(Color::Cyan).bg(if is_selected {
                            Color::Blue
                        } else {
                            Color::Black
                        }),
                    ));
                    row.push(Span::raw(" "));
                    row.push({
                        let mut s = status_span(msg.status);
                        if is_selected {
                            s = s.patch_style(Style::default().bg(Color::Blue));
                        }
                        s
                    });
                    lines.push(Line::from(row));
                } else {
                    let pad = area_w.saturating_sub(line_w + 2);
                    let mut row: Vec<Span> = vec![Span::raw(" ".repeat(pad))];
                    row.append(&mut spans);
                    row.push(Span::styled(
                        " ┃",
                        Style::default().fg(Color::Cyan).bg(if is_selected {
                            Color::Blue
                        } else {
                            Color::Black
                        }),
                    ));
                    lines.push(Line::from(row));
                }
            }
        } else {
            // ── Incoming: left-aligned, purple ┃ ─────────────────────────────
            let is_new = new_start_idx.map(|s| i >= s).unwrap_or(false);
            let name_color = if is_new {
                Color::Yellow
            } else {
                Color::Magenta
            };
            let bar_color = if is_new {
                Color::Yellow
            } else {
                Color::Magenta
            };
            let msg_color = if is_new { Color::White } else { Color::Gray };

            if is_group_start {
                let header_line = if is_selected {
                    Line::from(vec![
                        Span::styled("▌ ", Style::default().fg(Color::Cyan).bg(Color::Blue)),
                        Span::styled(
                            sender_display.clone(),
                            Style::default().fg(name_color).bg(Color::Blue),
                        ),
                        Span::styled(
                            format!(" {}", time),
                            Style::default().fg(Color::DarkGray).bg(Color::Blue),
                        ),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled("┃ ", Style::default().fg(bar_color)),
                        Span::styled(sender_display.clone(), Style::default().fg(name_color)),
                        Span::styled(format!(" {}", time), Style::default().fg(Color::DarkGray)),
                    ])
                };
                lines.push(header_line);
            }

            let content_w = area_w.saturating_sub(2); // "┃ " = 2 cols
            let content_text = msg.content.as_text().to_string();
            for original_line in content_text.split('\n') {
                for text_line in wrap_to_width(original_line, content_w) {
                    let bar = if is_selected {
                        Span::styled("▌ ", Style::default().fg(Color::Cyan).bg(Color::Blue))
                    } else {
                        Span::styled("┃ ", Style::default().fg(bar_color))
                    };
                    let mut content_spans = build_content_spans(&text_line, is_selected, msg_color);
                    let mut row = vec![bar];
                    row.append(&mut content_spans);
                    lines.push(Line::from(row));
                }
            }
        }
    } // end message loop

    // Padding so the last message is never clipped by word-wrap miscalculation
    lines.push(Line::from(""));
    lines.push(Line::from(""));
    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // Auto-scroll: each Line in `lines` is a pre-wrapped single terminal row
    // (Wrap is not set on the Paragraph, so no re-wrapping occurs at render time).
    let visible_height = area.height.saturating_sub(2) as usize;
    let total_lines = lines.len();
    let auto_scroll = if total_lines > visible_height {
        u16::try_from(total_lines - visible_height).unwrap_or(u16::MAX)
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
        .scroll((effective_scroll, 0));

    f.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::{display_width_of_status, split_line_with_urls, status_span, wrap_to_width};

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

    #[test]
    fn wrap_single_emoji_fits_at_width_2() {
        // "🎉" has display width 2; should fit on one line when max_w=2
        assert_eq!(wrap_to_width("🎉", 2), vec!["🎉"]);
    }

    #[test]
    fn wrap_emoji_string_each_on_own_line() {
        // Five emoji at width 2 each; max_w=2 → one per line
        assert_eq!(
            wrap_to_width("🎉🎊🎈🎁🎀", 2),
            vec!["🎉", "🎊", "🎈", "🎁", "🎀"]
        );
    }

    #[test]
    fn wrap_mixed_ascii_emoji() {
        // "hi 🎉" — "hi" is 2 cols, space+emoji pushes to 5 cols total; max_w=4
        // "hi" fits, "🎉" goes to next line
        assert_eq!(wrap_to_width("hi 🎉", 4), vec!["hi", "🎉"]);
    }

    #[test]
    fn wrap_emoji_does_not_split_mid_codepoint() {
        // "🎉🎊" at max_w=3 — "🎉" is width 2 (fits), "🎊" is width 2 (new line)
        assert_eq!(wrap_to_width("🎉🎊", 3), vec!["🎉", "🎊"]);
    }

    #[test]
    fn status_width_all_variants() {
        use crate::core::types::MessageStatus;
        assert_eq!(display_width_of_status(MessageStatus::Sending), 3);
        assert_eq!(display_width_of_status(MessageStatus::Sent), 1);
        assert_eq!(display_width_of_status(MessageStatus::Delivered), 2);
        assert_eq!(display_width_of_status(MessageStatus::Read), 2);
        assert_eq!(display_width_of_status(MessageStatus::Failed), 1);
    }

    #[test]
    fn status_span_content_and_color() {
        use crate::core::types::MessageStatus;
        use ratatui::style::Color;
        assert_eq!(status_span(MessageStatus::Sending).content.as_ref(), "···");
        assert_eq!(status_span(MessageStatus::Sent).content.as_ref(), "✓");
        assert_eq!(status_span(MessageStatus::Delivered).content.as_ref(), "✓✓");
        assert_eq!(status_span(MessageStatus::Read).content.as_ref(), "✓✓");
        assert_eq!(status_span(MessageStatus::Failed).content.as_ref(), "✗");
        assert_eq!(
            status_span(MessageStatus::Read).style.fg,
            Some(Color::Green)
        );
        assert_eq!(
            status_span(MessageStatus::Failed).style.fg,
            Some(Color::Red)
        );
    }

    #[test]
    fn pre_wrap_one_line_per_terminal_row() {
        // 50-char string at max_w=40 wraps to 2 lines
        let text = "a".repeat(50);
        let wrapped = wrap_to_width(&text, 40);
        assert_eq!(wrapped.len(), 2);
        // 50-char string at max_w=60 stays 1 line
        let wrapped2 = wrap_to_width(&text, 60);
        assert_eq!(wrapped2.len(), 1);
    }
}
