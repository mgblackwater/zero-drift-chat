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
    OpenSearch,
    SearchInput(KeyEvent),
    SearchNext,
    SearchPrev,
    SearchConfirm,
    SearchClose,
    AiSuggestAccept,   // Tab — accept ghost text suggestion
    AiSuggestRequest,  // Ctrl+Space — on-demand trigger
    None,
}

pub fn map_key(key: KeyEvent, mode: InputMode, enter_sends: bool) -> Action {
    match mode {
        InputMode::Normal   => map_normal_mode(key),
        InputMode::Editing  => map_editing_mode(key, enter_sends),
        InputMode::Settings => map_settings_mode(key),
        InputMode::Renaming => map_renaming_mode(key),
        InputMode::ChatMenu => map_chat_menu_mode(key),
        InputMode::Searching => map_search_mode(key),
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
        KeyCode::Char('/') => Action::OpenSearch,
        KeyCode::PageUp => Action::ScrollUp,
        KeyCode::PageDown => Action::ScrollDown,
        _ => Action::None,
    }
}

fn map_editing_mode(key: KeyEvent, enter_sends: bool) -> Action {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => Action::ExitEditing,

        // enter_sends=true (default): plain Enter submits, Shift/Alt+Enter inserts newline
        (KeyCode::Enter, m)
            if enter_sends && m == KeyModifiers::NONE => Action::SubmitMessage,
        (KeyCode::Enter, _)
            if enter_sends => Action::InputKey(key), // Shift/Alt+Enter → forward to textarea as newline

        // enter_sends=false: Shift/Alt+Enter submits, plain Enter inserts newline
        (KeyCode::Enter, m)
            if !enter_sends && m.contains(KeyModifiers::SHIFT) => Action::SubmitMessage,
        (KeyCode::Enter, m)
            if !enter_sends && m.contains(KeyModifiers::ALT) => Action::SubmitMessage,

        // Ctrl+J: WSL-friendly newline insert (Shift+Enter not reliably transmitted in WSL)
        (KeyCode::Char('j'), m) if m.contains(KeyModifiers::CONTROL) => {
            Action::InputKey(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        }
        // Ctrl+S always submits regardless of mode
        (KeyCode::Char('s'), m) if m.contains(KeyModifiers::CONTROL) => Action::SubmitMessage,
        // Ctrl+U always clears
        (KeyCode::Char('u'), m) if m.contains(KeyModifiers::CONTROL) => Action::ClearInput,
        (KeyCode::Tab, _) => Action::AiSuggestAccept,
        (KeyCode::Char(' '), m) if m.contains(KeyModifiers::CONTROL) => Action::AiSuggestRequest,
        // Everything else forwarded to TextArea
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

fn map_search_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::SearchClose,
        KeyCode::Enter => Action::SearchConfirm,
        KeyCode::Down => Action::SearchNext,
        KeyCode::Up => Action::SearchPrev,
        _ => Action::SearchInput(key),
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
