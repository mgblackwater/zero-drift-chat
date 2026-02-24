use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app_state::InputMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    SwitchPanel,
    NextChat,
    PrevChat,
    EnterEditing,
    ExitEditing,
    SubmitMessage,
    DeleteChar,
    InsertChar(char),
    ScrollUp,
    ScrollDown,
    None,
}

pub fn map_key(key: KeyEvent, mode: InputMode) -> Action {
    match mode {
        InputMode::Normal => map_normal_mode(key),
        InputMode::Editing => map_editing_mode(key),
    }
}

fn map_normal_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
        KeyCode::Tab => Action::SwitchPanel,
        KeyCode::Char('j') | KeyCode::Down => Action::NextChat,
        KeyCode::Char('k') | KeyCode::Up => Action::PrevChat,
        KeyCode::Char('i') | KeyCode::Enter => Action::EnterEditing,
        KeyCode::PageUp => Action::ScrollUp,
        KeyCode::PageDown => Action::ScrollDown,
        _ => Action::None,
    }
}

fn map_editing_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::ExitEditing,
        KeyCode::Enter => Action::SubmitMessage,
        KeyCode::Backspace => Action::DeleteChar,
        KeyCode::Char(c) => Action::InsertChar(c),
        _ => Action::None,
    }
}
