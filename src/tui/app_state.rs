use ratatui::widgets::ListState;
use tui_textarea::TextArea;

use crate::config::AppConfig;
use crate::core::types::{UnifiedChat, UnifiedMessage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing,
    Settings,
    Renaming,
    ChatMenu,
    Searching,
}

// --- Settings overlay types ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsKey {
    MockEnabled,
    WhatsAppEnabled,
    LogLevel,
    EnterSends,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsValue {
    Bool(bool),
    Choice(Vec<String>, usize),
}

#[derive(Debug, Clone)]
pub struct SettingsItem {
    pub key: SettingsKey,
    pub label: String,
    pub value: SettingsValue,
}

pub struct SettingsState {
    pub items: Vec<SettingsItem>,
    pub selected: usize,
    pub dirty: bool,
}

impl SettingsState {
    pub fn from_config(config: &AppConfig, enter_sends: bool) -> Self {
        let log_choices = vec![
            "trace".to_string(),
            "debug".to_string(),
            "info".to_string(),
            "warn".to_string(),
            "error".to_string(),
        ];
        let log_idx = log_choices
            .iter()
            .position(|l| l == &config.general.log_level)
            .unwrap_or(2);

        Self {
            items: vec![
                SettingsItem {
                    key: SettingsKey::MockEnabled,
                    label: "Mock Provider".to_string(),
                    value: SettingsValue::Bool(config.mock_provider.enabled),
                },
                SettingsItem {
                    key: SettingsKey::WhatsAppEnabled,
                    label: "WhatsApp".to_string(),
                    value: SettingsValue::Bool(config.whatsapp.enabled),
                },
                SettingsItem {
                    key: SettingsKey::LogLevel,
                    label: "Log Level".to_string(),
                    value: SettingsValue::Choice(log_choices, log_idx),
                },
                SettingsItem {
                    key: SettingsKey::EnterSends,
                    label: "Enter to Send".to_string(),
                    value: SettingsValue::Bool(enter_sends),
                },
            ],
            selected: 0,
            dirty: false,
        }
    }

    pub fn select_next(&mut self) {
        if self.selected < self.items.len() - 1 {
            self.selected += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn toggle_selected(&mut self) {
        if let Some(item) = self.items.get_mut(self.selected) {
            match &mut item.value {
                SettingsValue::Bool(ref mut b) => *b = !*b,
                SettingsValue::Choice(choices, ref mut idx) => {
                    *idx = (*idx + 1) % choices.len();
                }
            }
            self.dirty = true;
        }
    }

    pub fn apply_to_config(&self, config: &mut AppConfig) {
        for item in &self.items {
            match (&item.key, &item.value) {
                (SettingsKey::MockEnabled, SettingsValue::Bool(v)) => {
                    config.mock_provider.enabled = *v;
                }
                (SettingsKey::WhatsAppEnabled, SettingsValue::Bool(v)) => {
                    config.whatsapp.enabled = *v;
                }
                (SettingsKey::LogLevel, SettingsValue::Choice(choices, idx)) => {
                    config.general.log_level = choices[*idx].clone();
                }
                (SettingsKey::EnterSends, _) => {
                    // stored in SQLite preferences, not TOML config
                }
                _ => {}
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatMenuItem {
    TogglePin,
}

impl ChatMenuItem {
    pub fn label(&self, is_pinned: bool) -> &'static str {
        match self {
            ChatMenuItem::TogglePin => {
                if is_pinned { "Unpin" } else { "Pin" }
            }
        }
    }
}

pub struct ChatMenuState {
    pub chat_id: String,
    pub chat_name: String,
    pub is_pinned: bool,
    pub selected: usize,
    pub items: Vec<ChatMenuItem>,
}

impl ChatMenuState {
    pub fn new(chat_id: String, chat_name: String, is_pinned: bool) -> Self {
        Self {
            chat_id,
            chat_name,
            is_pinned,
            selected: 0,
            items: vec![ChatMenuItem::TogglePin],
        }
    }

    pub fn select_next(&mut self) {
        if self.selected < self.items.len() - 1 {
            self.selected += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchState {
    pub query: String,
    pub results: Vec<usize>,  // indices into AppState::chats (top 5)
    pub selected: usize,      // currently highlighted result index
}

impl SearchState {
    pub fn new() -> Self {
        Self { query: String::new(), results: vec![], selected: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePanel {
    ChatList,
    MessageView,
}

pub struct AppState {
    pub chats: Vec<UnifiedChat>,
    pub messages: Vec<UnifiedMessage>,
    pub chat_list_state: ListState,
    pub active_panel: ActivePanel,
    pub input_mode: InputMode,
    pub input: TextArea<'static>,
    pub scroll_offset: u16,
    pub should_quit: bool,
    pub qr_code: Option<String>,
    pub whatsapp_connected: bool,
    pub mock_enabled: bool,
    pub settings_state: Option<SettingsState>,
    pub chat_menu_state: Option<ChatMenuState>,
    pub search_state: Option<SearchState>,
    pub ai_suggestion: Option<String>,
    pub ai_status: Option<String>,
    pub ai_debug: bool,
    pub ai_debug_log: Vec<String>,
    pub enter_sends: bool,
    /// Number of unread messages at the tail of `messages` when a chat was opened.
    pub new_message_count: usize,
}

impl AppState {
    pub fn new() -> Self {
        let mut chat_list_state = ListState::default();
        chat_list_state.select(Some(0));

        Self {
            chats: Vec::new(),
            messages: Vec::new(),
            chat_list_state,
            active_panel: ActivePanel::ChatList,
            input_mode: InputMode::Normal,
            input: TextArea::default(),
            scroll_offset: 0,
            should_quit: false,
            qr_code: None,
            whatsapp_connected: false,
            mock_enabled: false,
            settings_state: None,
            chat_menu_state: None,
            search_state: None,
            ai_suggestion: None,
            ai_status: None,
            ai_debug: false,
            ai_debug_log: Vec::new(),
            enter_sends: true,
            new_message_count: 0,
        }
    }

    pub fn selected_chat_id(&self) -> Option<&str> {
        self.chat_list_state
            .selected()
            .and_then(|i| self.chats.get(i))
            .map(|c| c.id.as_str())
    }

    pub fn select_next_chat(&mut self) {
        if self.chats.is_empty() {
            return;
        }
        let i = match self.chat_list_state.selected() {
            Some(i) => {
                if i >= self.chats.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.chat_list_state.select(Some(i));
        self.scroll_offset = 0;
    }

    pub fn select_prev_chat(&mut self) {
        if self.chats.is_empty() {
            return;
        }
        let i = match self.chat_list_state.selected() {
            Some(i) => {
                if i == 0 {
                    0 // stay at top, no wrap
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.chat_list_state.select(Some(i));
        self.scroll_offset = 0;
    }

    pub fn switch_panel(&mut self) {
        self.active_panel = match self.active_panel {
            ActivePanel::ChatList => ActivePanel::MessageView,
            ActivePanel::MessageView => ActivePanel::ChatList,
        };
    }

    pub fn enter_editing(&mut self) {
        self.input_mode = InputMode::Editing;
        self.active_panel = ActivePanel::MessageView;
    }

    pub fn exit_editing(&mut self) {
        self.input_mode = InputMode::Normal;
    }

    pub fn take_input(&mut self) -> String {
        let text = self.input.lines().join("\n");
        self.input = TextArea::default();
        text
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(3);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(3);
    }

    pub fn open_settings(&mut self, config: &AppConfig, enter_sends: bool) {
        self.settings_state = Some(SettingsState::from_config(config, enter_sends));
        self.input_mode = InputMode::Settings;
    }

    pub fn close_settings(&mut self) {
        self.settings_state = None;
        self.input_mode = InputMode::Normal;
    }

    pub fn open_chat_menu(&mut self) {
        if let Some(idx) = self.chat_list_state.selected() {
            if let Some(chat) = self.chats.get(idx) {
                self.chat_menu_state = Some(ChatMenuState::new(
                    chat.id.clone(),
                    chat.display_name.clone().unwrap_or_else(|| chat.name.clone()),
                    chat.is_pinned,
                ));
                self.input_mode = InputMode::ChatMenu;
            }
        }
    }

    pub fn close_chat_menu(&mut self) {
        self.chat_menu_state = None;
        self.input_mode = InputMode::Normal;
    }

    pub fn push_ai_log(&mut self, entry: String) {
        if self.ai_debug {
            self.ai_debug_log.push(entry);
            if self.ai_debug_log.len() > 6 {
                self.ai_debug_log.remove(0);
            }
        }
    }

    pub fn has_unread(&self) -> bool {
        self.chats.iter().any(|c| c.unread_count > 0)
    }
}
