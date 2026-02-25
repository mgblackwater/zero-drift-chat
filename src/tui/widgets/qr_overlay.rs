use qrcode::QrCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// Render a QR code as a centered overlay popup.
/// Uses Unicode half-block characters (▀ ▄ █ and space) to render
/// two rows of QR modules per terminal row for compact display.
pub fn render_qr_overlay(f: &mut Frame, qr_string: &str) {
    let qr = match QrCode::new(qr_string.as_bytes()) {
        Ok(q) => q,
        Err(e) => {
            // If QR generation fails, show error overlay
            render_error_overlay(f, &format!("QR Error: {}", e));
            return;
        }
    };

    let modules = qr.to_colors();
    let width = qr.width();

    // Build lines using half-block rendering:
    // Each terminal row represents 2 QR module rows.
    // ▀ = top module dark, bottom module light
    // ▄ = top module light, bottom module dark
    // █ = both dark
    // ' ' = both light
    let mut lines: Vec<Line> = Vec::new();

    // Add quiet zone (1 row of whitespace above)
    let quiet_span = Span::styled(
        " ".repeat(width + 4),
        Style::default().fg(Color::White).bg(Color::White),
    );
    lines.push(Line::from(quiet_span.clone()));

    let row_count = width;
    let mut y = 0;
    while y < row_count {
        let mut spans = Vec::new();
        // Quiet zone left
        spans.push(Span::styled(
            "  ",
            Style::default().fg(Color::White).bg(Color::White),
        ));

        for x in 0..width {
            let top_dark = modules[y * width + x] == qrcode::Color::Dark;
            let bottom_dark = if y + 1 < row_count {
                modules[(y + 1) * width + x] == qrcode::Color::Dark
            } else {
                false
            };

            let (ch, fg, bg) = match (top_dark, bottom_dark) {
                (true, true) => ('█', Color::Black, Color::Black),
                (true, false) => ('▀', Color::Black, Color::White),
                (false, true) => ('▄', Color::Black, Color::White),
                (false, false) => (' ', Color::White, Color::White),
            };
            spans.push(Span::styled(
                ch.to_string(),
                Style::default().fg(fg).bg(bg),
            ));
        }

        // Quiet zone right
        spans.push(Span::styled(
            "  ",
            Style::default().fg(Color::White).bg(Color::White),
        ));
        lines.push(Line::from(spans));
        y += 2;
    }

    // Quiet zone bottom
    lines.push(Line::from(quiet_span));

    // Calculate overlay dimensions
    let qr_display_width = (width + 4) as u16; // QR + quiet zones
    let qr_display_height = (lines.len() + 4) as u16; // QR lines + border + instruction

    let area = f.area();
    let popup = centered_rect(qr_display_width + 2, qr_display_height + 2, area);

    // Clear the area behind the popup
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" WhatsApp — Scan QR Code ")
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    // Split inner into QR area and instruction text
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(inner);

    let qr_paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    f.render_widget(qr_paragraph, chunks[0]);

    let instruction = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Scan with WhatsApp to link this device",
            Style::default().fg(Color::DarkGray),
        )),
    ])
    .alignment(Alignment::Center);
    f.render_widget(instruction, chunks[1]);
}

fn render_error_overlay(f: &mut Frame, msg: &str) {
    let area = f.area();
    let popup = centered_rect(50, 5, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .title(" Error ")
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let paragraph = Paragraph::new(msg)
        .style(Style::default().fg(Color::Red))
        .alignment(Alignment::Center);
    f.render_widget(paragraph, inner);
}

/// Create a centered rectangle with fixed dimensions, clamped to the terminal area.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}
