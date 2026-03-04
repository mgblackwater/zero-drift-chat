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
    ClearInput,
    InputKey(KeyEvent),
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
    OpenChatMenu,
    ChatMenuNext,
    ChatMenuPrev,
    ChatMenuConfirm,
    ChatMenuClose,
    None,
}

pub fn map_key(key: KeyEvent, mode: InputMode) -> Action {
    match mode {
        InputMode::Normal => map_normal_mode(key),
        InputMode::Editing => map_editing_mode(key),
        InputMode::Settings => map_settings_mode(key),
        InputMode::Renaming => map_renaming_mode(key),
        InputMode::ChatMenu => map_chat_menu_mode(key),
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
        KeyCode::Char('x') => Action::OpenChatMenu,
        KeyCode::PageUp => Action::ScrollUp,
        KeyCode::PageDown => Action::ScrollDown,
        _ => Action::None,
    }
}

fn map_editing_mode(key: KeyEvent) -> Action {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => Action::ExitEditing,
        // Shift+Enter: works on Windows Terminal, iTerm2, modern macOS terminals, WSL
        (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => Action::SubmitMessage,
        // Alt+Enter: fallback for macOS Terminal.app and other terminals
        (KeyCode::Enter, m) if m.contains(KeyModifiers::ALT) => Action::SubmitMessage,
        // Ctrl+S: universal reliable fallback (works on all terminals including WSL)
        (KeyCode::Char('s'), m) if m.contains(KeyModifiers::CONTROL) => Action::SubmitMessage,
        // Ctrl+U: clear entire buffer (override tui-textarea default of undo)
        (KeyCode::Char('u'), m) if m.contains(KeyModifiers::CONTROL) => Action::ClearInput,
        // All other keys forwarded to TextArea
        _ => Action::InputKey(key),
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

fn map_chat_menu_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Action::ChatMenuNext,
        KeyCode::Char('k') | KeyCode::Up => Action::ChatMenuPrev,
        KeyCode::Enter | KeyCode::Char('p') => Action::ChatMenuConfirm,
        KeyCode::Esc | KeyCode::Char('q') => Action::ChatMenuClose,
        _ => Action::None,
    }
}

fn map_renaming_mode(key: KeyEvent) -> Action {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => Action::CancelRename,
        // Plain Enter confirms rename (single-line context)
        (KeyCode::Enter, m) if m == KeyModifiers::NONE => Action::ConfirmRename,
        // Block Shift+Enter / Alt+Enter from inserting newlines into a chat name
        (KeyCode::Enter, _) => Action::None,
        _ => Action::InputKey(key),
    }
}
