use std::collections::HashMap;
use std::time::Instant;

use ratatui::widgets::ListState;
use tui_textarea::TextArea;

use crate::config::AppConfig;
use crate::core::types::{Platform, UnifiedChat, UnifiedMessage};
use crate::storage::ScheduledMessage;

/// Tracks a contact who is currently typing in a chat.
#[derive(Debug, Clone)]
pub struct TypingInfo {
    #[allow(dead_code)]
    pub user_name: String,
    pub expires_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing,
    Settings,
    Renaming,
    ChatMenu,
    Searching,
    MessageSelect,
    SchedulePrompt,
    ScheduleList,
    TelegramAuth,
}

// --- Settings overlay types ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsKey {
    MockEnabled,
    WhatsAppEnabled,
    LogLevel,
    EnterSends,
    ActivityGraph,
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
                SettingsItem {
                    key: SettingsKey::ActivityGraph,
                    label: "Activity Graph".to_string(),
                    value: SettingsValue::Bool(true),
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
    ToggleMute,
}

impl ChatMenuItem {
    pub fn label(&self, is_pinned: bool, is_muted: bool) -> &'static str {
        match self {
            ChatMenuItem::TogglePin => {
                if is_pinned {
                    "Unpin"
                } else {
                    "Pin"
                }
            }
            ChatMenuItem::ToggleMute => {
                if is_muted {
                    "Unmute"
                } else {
                    "Mute"
                }
            }
        }
    }
}

pub struct ChatMenuState {
    pub chat_id: String,
    pub chat_name: String,
    pub is_pinned: bool,
    pub is_muted: bool,
    pub selected: usize,
    pub items: Vec<ChatMenuItem>,
}

impl ChatMenuState {
    pub fn new(chat_id: String, chat_name: String, is_pinned: bool, is_muted: bool) -> Self {
        Self {
            chat_id,
            chat_name,
            is_pinned,
            is_muted,
            selected: 0,
            items: vec![ChatMenuItem::TogglePin, ChatMenuItem::ToggleMute],
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
    pub results: Vec<usize>, // indices into AppState::chats (top 5)
    pub selected: usize,     // currently highlighted result index
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            results: vec![],
            selected: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SchedulePromptState {
    pub query: String,
    pub message_text: String, // the message to be scheduled
    pub chat_id: String,
    pub platform: Platform,
}

impl SchedulePromptState {
    pub fn new(message_text: String, chat_id: String, platform: Platform) -> Self {
        Self {
            query: String::new(),
            message_text,
            chat_id,
            platform,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScheduleListState {
    pub messages: Vec<ScheduledMessage>,
    pub selected: usize,
}

impl ScheduleListState {
    pub fn new(messages: Vec<ScheduledMessage>) -> Self {
        Self {
            messages,
            selected: 0,
        }
    }

    pub fn select_next(&mut self) {
        if !self.messages.is_empty() && self.selected < self.messages.len() - 1 {
            self.selected += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegramAuthStage {
    Phone,
    Otp,
    Password,
}

impl TelegramAuthStage {
    pub fn prompt(&self) -> &'static str {
        match self {
            TelegramAuthStage::Phone => {
                "Enter your Telegram phone number (with country code, e.g. +1234567890):"
            }
            TelegramAuthStage::Otp => "Enter the code Telegram sent you:",
            TelegramAuthStage::Password => "Enter your 2FA password:",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            TelegramAuthStage::Phone => " Telegram Login — Phone ",
            TelegramAuthStage::Otp => " Telegram Login — Code ",
            TelegramAuthStage::Password => " Telegram Login — 2FA Password ",
        }
    }

    pub fn is_password(&self) -> bool {
        matches!(self, TelegramAuthStage::Password)
    }
}

#[derive(Debug)]
pub struct TelegramAuthState {
    pub stage: TelegramAuthStage,
    pub input: String,
    /// Optional error hint from retry (e.g. "Wrong code, try again")
    pub error_hint: Option<String>,
}

impl TelegramAuthState {
    pub fn new(stage: TelegramAuthStage, error_hint: Option<String>) -> Self {
        Self {
            stage,
            input: String::new(),
            error_hint,
        }
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
    pub telegram_auth_state: Option<TelegramAuthState>,
    pub ai_suggestion: Option<String>,
    pub ai_status: Option<String>,
    pub ai_debug: bool,
    pub ai_debug_log: Vec<String>,
    pub enter_sends: bool,
    pub show_activity_graph: bool,
    /// Number of unread messages at the tail of `messages` when a chat was opened.
    pub new_message_count: usize,
    /// Transient copy feedback: Some("Copied!") briefly after y is pressed.
    pub copy_status: Option<String>,
    /// Index into `messages` of the currently highlighted message in MessageSelect mode.
    pub selected_message_idx: Option<usize>,
    pub schedule_prompt_state: Option<SchedulePromptState>,
    pub schedule_list_state: Option<ScheduleListState>,
    pub schedule_status: Option<String>, // flash message for scheduling feedback
    /// Per-chat typing indicators: chat_id → who is typing and when it expires.
    pub typing_states: HashMap<String, TypingInfo>,
    /// Running-light phase: 0=first dot lit, 1=middle, 2=last. Cycles every 2 ticks (~500ms/step).
    pub blink_phase: u8,
    /// Per-chat 24h activity cache: chat_id → [u32; 24].
    /// Index 23 = current hour, index 0 = 23 hours ago.
    pub activity_cache: std::collections::HashMap<String, [u32; 24]>,
    /// `tick_count` value when the last full SQL activity refresh ran.
    pub activity_last_refresh_tick: u64,
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
            telegram_auth_state: None,
            ai_suggestion: None,
            ai_status: None,
            ai_debug: false,
            ai_debug_log: Vec::new(),
            enter_sends: true,
            show_activity_graph: true,
            new_message_count: 0,
            copy_status: None,
            selected_message_idx: None,
            schedule_prompt_state: None,
            schedule_list_state: None,
            schedule_status: None,
            typing_states: HashMap::new(),
            blink_phase: 0,
            activity_cache: std::collections::HashMap::new(),
            activity_last_refresh_tick: 0,
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

    pub fn open_settings(&mut self, config: &AppConfig, enter_sends: bool, show_activity_graph: bool) {
        let mut s = SettingsState::from_config(config, enter_sends);
        if let Some(item) = s.items.iter_mut().find(|i| i.key == SettingsKey::ActivityGraph) {
            item.value = SettingsValue::Bool(show_activity_graph);
        }
        self.settings_state = Some(s);
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
                    chat.display_name
                        .clone()
                        .unwrap_or_else(|| chat.name.clone()),
                    chat.is_pinned,
                    chat.is_muted,
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
        self.chats.iter().any(|c| !c.is_muted && c.unread_count > 0)
    }

    /// Enter message-selection mode, starting at the last message.
    pub fn enter_message_select(&mut self) {
        if self.messages.is_empty() {
            return;
        }
        self.selected_message_idx = Some(self.messages.len() - 1);
        self.input_mode = InputMode::MessageSelect;
        self.active_panel = ActivePanel::MessageView;
    }

    /// Exit message-selection mode.
    pub fn exit_message_select(&mut self) {
        self.selected_message_idx = None;
        self.input_mode = InputMode::Normal;
    }

    /// Move selection up (toward older messages).
    pub fn message_select_prev(&mut self) {
        if let Some(idx) = self.selected_message_idx {
            if idx > 0 {
                self.selected_message_idx = Some(idx - 1);
            }
        }
    }

    /// Move selection down (toward newer messages).
    pub fn message_select_next(&mut self) {
        if let Some(idx) = self.selected_message_idx {
            if idx + 1 < self.messages.len() {
                self.selected_message_idx = Some(idx + 1);
            }
        }
    }

    pub fn open_telegram_auth(&mut self, stage: TelegramAuthStage, error_hint: Option<String>) {
        self.telegram_auth_state = Some(TelegramAuthState::new(stage, error_hint));
        self.input_mode = InputMode::TelegramAuth;
    }

    pub fn close_telegram_auth(&mut self) {
        self.telegram_auth_state = None;
        self.input_mode = InputMode::Normal;
    }

    pub fn take_telegram_auth_input(&mut self) -> String {
        if let Some(ref mut auth) = self.telegram_auth_state {
            std::mem::take(&mut auth.input)
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_app_state_new_has_empty_typing_states() {
        let state = AppState::new();
        assert!(state.typing_states.is_empty());
        assert_eq!(state.blink_phase, 0);
    }

    #[test]
    fn test_typing_info_expiry() {
        let expired = TypingInfo {
            user_name: "Alice".to_string(),
            expires_at: Instant::now() - Duration::from_secs(1),
        };
        let active = TypingInfo {
            user_name: "Bob".to_string(),
            expires_at: Instant::now() + Duration::from_secs(5),
        };
        let now = Instant::now();
        assert!(expired.expires_at <= now, "expired entry should be in the past");
        assert!(active.expires_at > now, "active entry should be in the future");
    }

    #[test]
    fn test_blink_phase_cycles_0_1_2_0() {
        // Simulate the tick handler: advance by (phase + 1) % 3 and verify full wrap-around.
        let mut phase: u8 = 0;
        phase = (phase + 1) % 3;
        assert_eq!(phase, 1);
        phase = (phase + 1) % 3;
        assert_eq!(phase, 2);
        phase = (phase + 1) % 3;
        assert_eq!(phase, 0, "phase should wrap back to 0 after reaching 2");
    }
}
