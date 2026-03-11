use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app_state::InputMode;

pub fn render_status_bar(
    f: &mut Frame,
    area: Rect,
    mode: InputMode,
    enter_sends: bool,
    mock_enabled: bool,
    whatsapp_connected: bool,
    copy_status: Option<&str>,
    schedule_status: Option<&str>,
) {
    let hints = match mode {
        InputMode::Normal => "q:Quit | i:Insert | s:Settings | r:Rename | x:Menu | y:Copy last | v:Select msg | Tab:Switch",
        InputMode::Editing => {
            if enter_sends {
                "Esc:Normal | Enter:Send | Shift+Enter/Ctrl+J:Newline | Ctrl+S:Send | Ctrl+U:Clear"
            } else {
                "Esc:Normal | Enter:Newline | Shift+Enter/Ctrl+S:Send | Ctrl+U:Clear"
            }
        }
        InputMode::Settings => "j/k:Navigate | Enter/Space:Toggle | Ctrl+s:Save | Esc:Cancel",
        InputMode::Renaming => "Enter:Confirm | Esc:Cancel | Type new name",
        InputMode::ChatMenu => "j/k:Navigate | p/Enter:Confirm | Esc:Close",
        InputMode::Searching => "Type to filter | j/k:Navigate | Enter:Open+Insert | Esc:Cancel",
        InputMode::MessageSelect => "j/k:Navigate | y/Enter:Copy | Esc:Cancel",
        InputMode::SchedulePrompt => "Type time (e.g. 'tomorrow 9am', 'fri 3pm', 'Mar 15 14:30') | Enter:Confirm | Esc:Cancel",
        InputMode::ScheduleList => "j/k:Navigate | d:Cancel | Esc/q:Close",
    };

    // Mode pill: colored badge on the left, rest of bar stays on black
    let (pill_label, pill_bg, pill_fg) = match mode {
        InputMode::Normal => (" NORMAL ", Color::DarkGray, Color::Black),
        InputMode::Editing => (" ✏ INSERT ", Color::Yellow, Color::Black),
        InputMode::Settings => (" SETTINGS ", Color::Cyan, Color::Black),
        InputMode::Renaming => (" RENAME ", Color::Magenta, Color::Black),
        InputMode::ChatMenu => (" MENU ", Color::Yellow, Color::Black),
        InputMode::Searching => (" SEARCH ", Color::Cyan, Color::Black),
        InputMode::MessageSelect => (" SELECT ", Color::Blue, Color::White),
        InputMode::SchedulePrompt => (" SCHEDULE ", Color::Green, Color::Black),
        InputMode::ScheduleList => (" SCHEDULED ", Color::Green, Color::Black),
    };

    let sep = Style::default().fg(Color::DarkGray);

    let mut spans = vec![
        Span::styled(
            pill_label,
            Style::default()
                .bg(pill_bg)
                .fg(pill_fg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", sep),
        Span::styled(
            format!("v{} ", env!("CARGO_PKG_VERSION")),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(" │ ", sep),
    ];

    if mock_enabled {
        spans.push(Span::styled(" ● ", Style::default().fg(Color::Green)));
        spans.push(Span::styled("Mock", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(" │ ", sep));
    }

    let wa_color = if whatsapp_connected {
        Color::Green
    } else {
        Color::Red
    };
    spans.push(Span::styled(" ● ", Style::default().fg(wa_color)));
    spans.push(Span::styled("WA", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(" │ ", sep));

    // Show schedule feedback (highest priority), then copy feedback, then normal hints
    if let Some(sched) = schedule_status {
        let color = if sched.starts_with("Could not") {
            Color::Red
        } else {
            Color::Green
        };
        spans.push(Span::styled(
            sched,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    } else if let Some(status) = copy_status {
        spans.push(Span::styled(
            status,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled(hints, Style::default().fg(Color::DarkGray)));
    }

    let paragraph = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
    f.render_widget(paragraph, area);
}
