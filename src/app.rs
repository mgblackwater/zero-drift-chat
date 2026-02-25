use std::io;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::config::AppConfig;
use crate::core::provider::ProviderEvent;
use crate::core::types::{AuthStatus, MessageContent, Platform};
use crate::core::MessageRouter;
use crate::providers::mock::MockProvider;
use crate::providers::whatsapp::WhatsAppProvider;
use crate::storage::Database;
use crate::tui::app_state::AppState;
use crate::tui::event::{AppEvent, EventHandler};
use crate::tui::keybindings::{map_key, Action};
use crate::tui::render;

pub struct App {
    state: AppState,
    router: MessageRouter,
    db: Database,
    config: AppConfig,
}

impl App {
    pub fn new(config: AppConfig, db: Database) -> Self {
        Self {
            state: AppState::new(),
            router: MessageRouter::new(),
            db,
            config,
        }
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        // Register providers
        if self.config.mock_provider.enabled {
            let mock = MockProvider::new(
                self.config.mock_provider.chat_count,
                self.config.mock_provider.message_interval_secs,
            );
            self.router.register_provider(Box::new(mock));
        }

        if self.config.whatsapp.enabled {
            let session_path = format!(
                "{}/whatsapp-session.db",
                self.config.general.data_dir
            );
            let wa = WhatsAppProvider::new(session_path);
            self.router.register_provider(Box::new(wa));
        }

        // Start all providers
        self.router.start_all().await?;

        // Load persisted chats from DB
        if let Ok(chats) = self.db.get_all_chats() {
            if !chats.is_empty() {
                self.state.chats = chats;
            }
        }

        // Set up terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Set panic hook to restore terminal
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
            original_hook(panic_info);
        }));

        // Event handler
        let mut events = EventHandler::new(
            self.config.tui.tick_rate_ms,
            self.config.tui.render_rate_ms,
        );

        // Main loop
        loop {
            let event = events.next().await;
            match event {
                Some(AppEvent::Render) => {
                    terminal.draw(|f| render::draw(f, &mut self.state))?;
                }
                Some(AppEvent::Tick) => {
                    self.handle_tick();
                }
                Some(AppEvent::Key(key)) => {
                    let action = map_key(key, self.state.input_mode);
                    self.handle_action(action).await;
                    if self.state.should_quit {
                        break;
                    }
                }
                Some(AppEvent::Resize(_, _)) => {
                    // Terminal handles resize automatically
                }
                Some(AppEvent::Quit) | None => {
                    break;
                }
            }
        }

        // Cleanup
        self.router.stop_all().await?;
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        Ok(())
    }

    fn handle_tick(&mut self) {
        let events = self.router.poll_events();

        // Cap events per tick to avoid blocking the render loop
        let max_events = 50;
        for event in events.into_iter().take(max_events) {
            match event {
                ProviderEvent::NewMessage(msg) => {
                    // Persist to DB
                    if let Err(e) = self.db.insert_message(&msg) {
                        tracing::error!("Failed to insert message: {}", e);
                    }

                    // Update last_message on the chat
                    let preview = msg.content.as_text().to_string();
                    let _ = self.db.update_last_message(&msg.chat_id, &preview);

                    // Update unread count if not viewing this chat
                    let is_current_chat = self
                        .state
                        .selected_chat_id()
                        .map(|id| id == msg.chat_id)
                        .unwrap_or(false);

                    if !is_current_chat && !msg.is_outgoing {
                        if let Some(chat) = self
                            .state
                            .chats
                            .iter_mut()
                            .find(|c| c.id == msg.chat_id)
                        {
                            chat.unread_count += 1;
                            let _ = self
                                .db
                                .update_unread_count(&chat.id, chat.unread_count);
                        }
                    }

                    // Update last message preview
                    if let Some(chat) = self
                        .state
                        .chats
                        .iter_mut()
                        .find(|c| c.id == msg.chat_id)
                    {
                        chat.last_message = Some(preview);
                    }

                    // Add to current view if it's the selected chat
                    if is_current_chat {
                        self.state.messages.push(msg);
                        self.state.scroll_offset = 0; // auto-scroll to bottom
                    }
                }
                ProviderEvent::ChatsUpdated(chats) => {
                    // Persist and merge — but skip expensive DB reads
                    for chat in &chats {
                        if let Err(e) = self.db.upsert_chat(chat) {
                            tracing::error!("Failed to upsert chat: {}", e);
                        }
                    }

                    // Merge: add new chats, update existing ones
                    for chat in chats {
                        if let Some(existing) = self
                            .state
                            .chats
                            .iter_mut()
                            .find(|c| c.id == chat.id)
                        {
                            if chat.last_message.is_some() {
                                existing.last_message = chat.last_message;
                            }
                            // Only update name if the new name looks like a real name
                            // (not a raw phone number) or if existing is still a number
                            let existing_is_numeric = existing.name.chars().all(|c| c.is_ascii_digit() || c == '+');
                            let new_is_numeric = chat.name.chars().all(|c| c.is_ascii_digit() || c == '+');
                            if !new_is_numeric || existing_is_numeric {
                                existing.name = chat.name;
                            }
                        } else {
                            self.state.chats.push(chat);
                        }
                    }
                    // Note: no load_selected_chat_messages() here —
                    // messages arrive via NewMessage events instead
                }
                ProviderEvent::MessageStatusUpdate { message_id, status } => {
                    let _ = self.db.update_message_status(&message_id, status);
                    if let Some(msg) = self
                        .state
                        .messages
                        .iter_mut()
                        .find(|m| m.id == message_id)
                    {
                        msg.status = status;
                    }
                }
                ProviderEvent::AuthStatusChanged(platform, status) => {
                    tracing::info!("Auth status changed for {:?}: {:?}", platform, status);
                    if platform == Platform::WhatsApp {
                        match status {
                            AuthStatus::Authenticated => {
                                self.state.whatsapp_connected = true;
                                self.state.qr_code = None;
                            }
                            AuthStatus::NotAuthenticated | AuthStatus::Failed => {
                                self.state.whatsapp_connected = false;
                            }
                            _ => {}
                        }
                    }
                }
                ProviderEvent::AuthQrCode(code) => {
                    tracing::info!("QR code received for WhatsApp pairing");
                    self.state.qr_code = Some(code);
                }
            }
        }
    }

    async fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.state.should_quit = true;
            }
            Action::SwitchPanel => {
                self.state.switch_panel();
            }
            Action::NextChat => {
                self.state.select_next_chat();
                self.load_selected_chat_messages();
                // Clear unread for selected chat
                self.clear_selected_unread();
            }
            Action::PrevChat => {
                self.state.select_prev_chat();
                self.load_selected_chat_messages();
                self.clear_selected_unread();
            }
            Action::EnterEditing => {
                self.state.enter_editing();
            }
            Action::ExitEditing => {
                self.state.exit_editing();
            }
            Action::SubmitMessage => {
                let input = self.state.take_input();
                if !input.is_empty() {
                    if let Some(chat_id) = self.state.selected_chat_id().map(|s| s.to_string()) {
                        // Determine which provider owns this chat
                        let platform = self
                            .state
                            .chats
                            .iter()
                            .find(|c| c.id == chat_id)
                            .map(|c| c.platform)
                            .unwrap_or(Platform::Mock);

                        if let Some(provider) = self.router.get_provider_mut(platform) {
                            match provider
                                .send_message(&chat_id, MessageContent::Text(input))
                                .await
                            {
                                Ok(_) => {
                                    tracing::debug!("Message sent via {:?}", platform);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to send message: {}", e);
                                }
                            }
                        }
                    }
                }
            }
            Action::DeleteChar => {
                self.state.delete_char();
            }
            Action::InsertChar(c) => {
                self.state.push_char(c);
            }
            Action::ScrollUp => {
                self.state.scroll_up();
            }
            Action::ScrollDown => {
                self.state.scroll_down();
            }
            Action::None => {}
        }
    }

    fn load_selected_chat_messages(&mut self) {
        if let Some(chat_id) = self.state.selected_chat_id().map(|s| s.to_string()) {
            match self.db.get_messages_for_chat(&chat_id) {
                Ok(messages) => {
                    self.state.messages = messages;
                    self.state.scroll_offset = 0;
                }
                Err(e) => {
                    tracing::error!("Failed to load messages: {}", e);
                }
            }
        }
    }

    fn clear_selected_unread(&mut self) {
        if let Some(idx) = self.state.chat_list_state.selected() {
            if let Some(chat) = self.state.chats.get_mut(idx) {
                if chat.unread_count > 0 {
                    chat.unread_count = 0;
                    let _ = self.db.update_unread_count(&chat.id, 0);
                }
            }
        }
    }
}
