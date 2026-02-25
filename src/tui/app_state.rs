use ratatui::widgets::ListState;

use crate::core::types::{UnifiedChat, UnifiedMessage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing,
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
    pub input_buffer: String,
    pub cursor_position: usize,
    pub scroll_offset: u16,
    pub should_quit: bool,
    pub qr_code: Option<String>,
    pub whatsapp_connected: bool,
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
            input_buffer: String::new(),
            cursor_position: 0,
            scroll_offset: 0,
            should_quit: false,
            qr_code: None,
            whatsapp_connected: false,
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
                    self.chats.len() - 1
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

    pub fn push_char(&mut self, c: char) {
        self.input_buffer.insert(self.cursor_position, c);
        self.cursor_position += c.len_utf8();
    }

    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            let prev = self.input_buffer[..self.cursor_position]
                .chars()
                .last()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            self.cursor_position -= prev;
            self.input_buffer.remove(self.cursor_position);
        }
    }

    pub fn take_input(&mut self) -> String {
        self.cursor_position = 0;
        std::mem::take(&mut self.input_buffer)
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(3);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(3);
    }
}
