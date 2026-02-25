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
    OpenSettings,
    SettingsNext,
    SettingsPrev,
    SettingsToggle,
    SettingsSave,
    SettingsClose,
    RenameChat,
    ConfirmRename,
    CancelRename,
    None,
}

pub fn map_key(key: KeyEvent, mode: InputMode) -> Action {
    match mode {
        InputMode::Normal => map_normal_mode(key),
        InputMode::Editing => map_editing_mode(key),
        InputMode::Settings => map_settings_mode(key),
        InputMode::Renaming => map_renaming_mode(key),
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
        KeyCode::Char('s') => Action::OpenSettings,
        KeyCode::Char('r') => Action::RenameChat,
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

fn map_settings_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Action::SettingsNext,
        KeyCode::Char('k') | KeyCode::Up => Action::SettingsPrev,
        KeyCode::Enter | KeyCode::Char(' ') => Action::SettingsToggle,
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::SettingsSave,
        KeyCode::Esc | KeyCode::Char('q') => Action::SettingsClose,
        _ => Action::None,
    }
}

fn map_renaming_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::CancelRename,
        KeyCode::Enter => Action::ConfirmRename,
        KeyCode::Backspace => Action::DeleteChar,
        KeyCode::Char(c) => Action::InsertChar(c),
        _ => Action::None,
    }
}
