use std::io::{self, Write};

use crossterm::{cursor::MoveTo, style::Print, execute};
use ratatui::{buffer::Buffer, style::Color};

/// After `terminal.draw()` flushes, scan the completed buffer for contiguous
/// Color::LightBlue cell runs (URL spans), reconstruct the URL text, then
/// re-print those characters to stdout wrapped in OSC 8 hyperlink sequences.
///
/// This approach avoids modifying ratatui Cell symbols (which would corrupt
/// ratatui's unicode-width based diff algorithm) and instead writes directly
/// to stdout after the frame has been flushed.
pub fn inject_osc8_hyperlinks(buffer: &Buffer) -> io::Result<()> {
    let area = buffer.area;
    let mut stdout = io::stdout().lock();

    for row in area.top()..area.bottom() {
        let mut col = area.left();
        while col < area.right() {
            if buffer[(col, row)].fg != Color::LightBlue {
                col += 1;
                continue;
            }
            // Collect the full text of this contiguous blue run
            let run_start = col;
            let mut url = String::new();
            while col < area.right() && buffer[(col, row)].fg == Color::LightBlue {
                url.push_str(buffer[(col, row)].symbol());
                col += 1;
            }

            // Only inject for complete URLs (wrapped fragments won't start with http)
            if !url.starts_with("http://") && !url.starts_with("https://") {
                continue;
            }

            // Move to the URL's position and re-print it with OSC 8 wrapping.
            // The characters are visually identical; the terminal now associates
            // the URL metadata with them so Ctrl+Click opens the browser.
            execute!(
                stdout,
                MoveTo(run_start, row),
                Print(format!("\x1B]8;;{url}\x07{url}\x1B]8;;\x07"))
            )?;
        }
    }

    stdout.flush()
}
