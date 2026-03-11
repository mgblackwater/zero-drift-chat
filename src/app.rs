use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::{
    execute,
    event::{DisableBracketedPaste, EnableBracketedPaste},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::ai::worker::{AiWorker, AiRequest};
use crate::ai::context::RawMessage;
use crate::ai::providers::openai::OpenAiClient;
use crate::ai::providers::anthropic::AnthropicClient;
use crate::ai::providers::gemini::GeminiClient;
use crate::config::AppConfig;
use crate::core::provider::ProviderEvent;
use crate::core::types::{AuthStatus, MessageContent, Platform};
use crate::core::MessageRouter;
use crate::providers::mock::MockProvider;
use crate::providers::whatsapp::WhatsAppProvider;
use crate::storage::{AddressBook, Database};
use tui_textarea::TextArea;

use crate::tui::app_state::{AppState, ChatMenuItem, InputMode, SearchState, SettingsKey, SettingsValue};
use crate::tui::event::{AppEvent, EventHandler};
use crate::tui::keybindings::{map_key, Action};
use crate::tui::render;
use crate::tui::search::top_fuzzy_matches;
use crate::tui;

pub struct App {
    state: AppState,
    router: MessageRouter,
    db: Database,
    address_book: AddressBook,
    config: AppConfig,
    config_path: PathBuf,
    ai_worker: Option<AiWorker>,
    last_keystroke: Option<Instant>,
    db_summary_tx: tokio::sync::mpsc::UnboundedSender<(String, String)>,
    db_summary_rx: tokio::sync::mpsc::UnboundedReceiver<(String, String)>,
}

impl App {
    pub fn new(
        config: AppConfig,
        db: Database,
        address_book: AddressBook,
        config_path: PathBuf,
        event_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    ) -> Self {
        tracing::info!(
            enabled = config.ai.enabled,
            provider = %config.ai.provider,
            base_url = %config.ai.base_url,
            model = %config.ai.model,
            debug = config.ai.debug,
            "AI config loaded"
        );
        let ai_worker = if config.ai.enabled {
            let provider: Box<dyn crate::ai::providers::AiProvider> = match config.ai.provider.as_str() {
                "anthropic" => Box::new(AnthropicClient::new(config.ai.api_key.clone())),
                "gemini"    => Box::new(GeminiClient::new(config.ai.api_key.clone())),
                _           => Box::new(OpenAiClient::new(config.ai.base_url.clone(), config.ai.api_key.clone())),
            };
            tracing::info!("AI worker created — autocomplete enabled");
            Some(AiWorker::new(provider, config.ai.clone(), event_tx))
        } else {
            tracing::info!("AI worker NOT created — ai.enabled = false in config");
            None
        };

        let (db_summary_tx, db_summary_rx) = tokio::sync::mpsc::unbounded_channel::<(String, String)>();

        let mut state = AppState::new();
        state.ai_debug = config.ai.debug;

        Self {
            state,
            router: MessageRouter::new(),
            db,
            address_book,
            config,
            config_path,
            ai_worker,
            last_keystroke: None,
            db_summary_tx,
            db_summary_rx,
        }
    }

    pub async fn run(&mut self, mut events: EventHandler) -> anyhow::Result<()> {
        // Load enter_sends preference from DB (default true)
        self.state.enter_sends = self.db
            .get_preference("enter_sends")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(true);

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

        // Track which providers are enabled for status bar
        self.state.mock_enabled = self.config.mock_provider.enabled;

        // Load persisted chats from DB, filtering out disabled providers
        if let Ok(chats) = self.db.get_all_chats() {
            let mut chats: Vec<_> = chats
                .into_iter()
                .filter(|c| match c.platform {
                    Platform::Mock => self.config.mock_provider.enabled,
                    Platform::WhatsApp => self.config.whatsapp.enabled,
                    _ => true,
                })
                .collect();

            // Apply display names from address book (user-set, highest priority)
            if let Ok(names) = self.address_book.get_all_display_names() {
                for chat in &mut chats {
                    if let Some(name) = names.get(&chat.id) {
                        chat.display_name = Some(name.clone());
                    }
                }
            }
            // Apply contact names to chats that still show a raw phone number
            for chat in &mut chats {
                if chat.display_name.is_some() {
                    continue;
                }
                if let Some(phone) = Self::extract_wa_phone(&chat.id) {
                    let is_numeric = chat.name.chars().all(|c| c.is_ascii_digit() || c == '+');
                    if is_numeric {
                        if let Ok(Some(contact_name)) = self.address_book.lookup_contact(phone) {
                            chat.name = contact_name;
                        }
                    }
                }
            }

            if !chats.is_empty() {
                self.state.chats = chats;
            }
        }

        // Load messages for the initially selected chat
        self.load_selected_chat_messages();

        // Set initial terminal title
        self.refresh_title();

        // Set up terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Set panic hook to restore terminal
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), DisableBracketedPaste, LeaveAlternateScreen);
            original_hook(panic_info);
        }));

        // Main loop
        loop {
            let event = events.next().await;
            match event {
                Some(AppEvent::Render) => {
                    let completed = terminal.draw(|f| render::draw(f, &mut self.state))?;
                    tui::osc8::inject_osc8_hyperlinks(completed.buffer)?;
                }
                Some(AppEvent::Tick) => {
                    self.handle_tick();
                    // Save any pending AI summaries to DB
                    while let Ok((key, value)) = self.db_summary_rx.try_recv() {
                        let _ = self.db.set_preference(&key, &value);
                    }
                    // AI debounce: fire request after user stops typing
                    if let Some(t) = self.last_keystroke {
                        let debounce = Duration::from_millis(self.config.ai.debounce_ms);
                        if t.elapsed() >= debounce {
                            self.last_keystroke = None;
                            if self.state.input_mode == InputMode::Editing {
                                let partial = self.state.input.lines().join("\n");
                                if !partial.is_empty() {
                                    if let Some(worker) = self.ai_worker.as_mut() {
                                        let messages: Vec<RawMessage> = self.state.messages.iter()
                                            .filter_map(|m| {
                                                let text = match &m.content {
                                                    crate::core::types::MessageContent::Text(t) => t.clone(),
                                                    _ => return None,
                                                };
                                                Some(RawMessage { is_outgoing: m.is_outgoing, text })
                                            })
                                            .collect();
                                        let summary = self.state.selected_chat_id()
                                            .and_then(|id| self.db.get_preference(&format!("ai_summary:{}", id)).ok().flatten());
                                        self.state.push_ai_log(format!(
                                            "[debounce] → POST {}/v1/chat/completions | model={} | ctx={} msgs | input={:?}",
                                            self.config.ai.base_url, self.config.ai.model, messages.len(),
                                            if partial.len() > 40 { &partial[..40] } else { &partial }
                                        ));
                                        tracing::info!(
                                            trigger = "debounce",
                                            url = %format!("{}/v1/chat/completions", self.config.ai.base_url),
                                            model = %self.config.ai.model,
                                            context_msgs = messages.len(),
                                            input = %partial,
                                            "AI autocomplete request"
                                        );
                                        worker.request(AiRequest { partial_input: partial, messages, summary });
                                    }
                                }
                            }
                        }
                    }
                }
                Some(AppEvent::Key(key)) => {
                    let action = map_key(key, self.state.input_mode, self.state.enter_sends);
                    self.handle_action(action).await;
                    if self.state.should_quit {
                        break;
                    }
                }
                Some(AppEvent::Resize(_, _)) => {
                    // Terminal handles resize automatically
                }
                Some(AppEvent::Paste(text)) => {
                    if self.state.input_mode == InputMode::Editing {
                        self.state.input.insert_str(&text);
                        self.state.ai_suggestion = None;
                        if self.ai_worker.is_some() {
                            self.last_keystroke = Some(Instant::now());
                        }
                    }
                }
                Some(AppEvent::AiSuggestion(text)) => {
                    if self.state.input_mode == InputMode::Editing {
                        tracing::info!(suggestion = %text, "AI autocomplete suggestion received");
                        self.state.push_ai_log(format!("[suggestion] ← {:?}", text));
                        self.state.ai_suggestion = Some(text);
                        self.state.ai_status = None;
                    }
                }
                Some(AppEvent::AiError(e)) => {
                    tracing::info!(error = %e, "AI autocomplete error");
                    self.state.push_ai_log(format!("[error] ← {}", e));
                    self.state.ai_status = Some(format!("AI: {}", e));
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
            DisableBracketedPaste,
            LeaveAlternateScreen
        )?;
        Self::update_title(false);   // restore clean title on exit
        terminal.show_cursor()?;

        Ok(())
    }

    fn handle_tick(&mut self) {
        // Clear transient copy status after one tick so it disappears quickly
        self.state.copy_status = None;

        let events = self.router.poll_events();

        // Cap events per tick to avoid blocking the render loop
        let max_events = 500;
        for event in events.into_iter().take(max_events) {
            match event {
                ProviderEvent::NewMessage(msg) => {
                    // Persist to DB
                    if let Err(e) = self.db.insert_message(&msg) {
                        tracing::error!("Failed to insert message: {}", e);
                    }

                    // Store push_name as contact so we can name phone-number chats
                    if !msg.is_outgoing && msg.sender != "You" && !msg.sender.is_empty() {
                        if let Some(phone) = Self::extract_wa_phone(&msg.chat_id) {
                            let _ = self.address_book.upsert_contact(phone, &msg.sender);
                            // Apply to the in-memory chat if it still shows a phone number
                            if let Some(chat) = self.state.chats.iter_mut().find(|c| c.id == msg.chat_id) {
                                if chat.display_name.is_none() {
                                    let is_numeric = chat.name.chars().all(|c| c.is_ascii_digit() || c == '+');
                                    if is_numeric {
                                        chat.name = msg.sender.clone();
                                    }
                                }
                            }
                        }
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
                            self.refresh_title();
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
                        self.state.messages.push(msg.clone());
                        self.state.scroll_offset = 0; // auto-scroll to bottom

                        if let Some(worker) = &self.ai_worker {
                            if let Some(chat_id) = self.state.selected_chat_id() {
                                let messages: Vec<crate::ai::context::RawMessage> = self.state.messages.iter()
                                    .filter_map(|m| {
                                        let text = match &m.content {
                                            crate::core::types::MessageContent::Text(t) => t.clone(),
                                            _ => return None,
                                        };
                                        Some(crate::ai::context::RawMessage { is_outgoing: m.is_outgoing, text })
                                    })
                                    .collect();
                                worker.maybe_generate_summary(
                                    chat_id.to_string(),
                                    messages,
                                    self.config.ai.summary_threshold,
                                    self.db_summary_tx.clone(),
                                );
                            }
                        }
                    }

                    // Move chat to top of its group (pinned→top of pinned, unpinned→top of unpinned)
                    if let Some(pos) = self.state.chats.iter().position(|c| c.id == msg.chat_id) {
                        let selected_id = self.state.selected_chat_id().map(|s| s.to_string());
                        let chat = self.state.chats.remove(pos);
                        let insert_pos = if chat.is_pinned {
                            0
                        } else {
                            // Top of unpinned section = one after the last pinned chat
                            self.state.chats.iter().rposition(|c| c.is_pinned).map(|p| p + 1).unwrap_or(0)
                        };
                        if pos != insert_pos {
                            self.state.chats.insert(insert_pos, chat);
                            if let Some(id) = selected_id {
                                if let Some(new_pos) = self.state.chats.iter().position(|c| c.id == id) {
                                    self.state.chat_list_state.select(Some(new_pos));
                                }
                            }
                        } else {
                            self.state.chats.insert(pos, chat);
                        }
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
                            // Don't touch display_name — it's user-set
                            // Only update provider name if it looks better
                            let existing_is_numeric = existing.name.chars().all(|c| c.is_ascii_digit() || c == '+');
                            let new_is_numeric = chat.name.chars().all(|c| c.is_ascii_digit() || c == '+');
                            if !new_is_numeric || existing_is_numeric {
                                existing.name = chat.name;
                            }
                        } else {
                            // Apply address-book / contact name to newly discovered chats
                            let mut new_chat = chat;
                            if let Some(name) = self.resolve_contact_name(&new_chat.id) {
                                new_chat.display_name = Some(name);
                            } else if let Some(phone) = Self::extract_wa_phone(&new_chat.id) {
                                if let Ok(Some(contact_name)) = self.address_book.lookup_contact(phone) {
                                    new_chat.name = contact_name;
                                }
                            }
                            self.state.chats.push(new_chat);
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
                ProviderEvent::SelfRead { chat_id } => {
                    if let Some(chat) = self.state.chats.iter_mut().find(|c| c.id == chat_id) {
                        if chat.unread_count > 0 {
                            chat.unread_count = 0;
                            let _ = self.db.update_unread_count(&chat.id, 0);
                        }
                    }
                    if self.state.selected_chat_id().map(|id| id == chat_id).unwrap_or(false) {
                        self.state.new_message_count = 0;
                    }
                    self.refresh_title();
                }
                ProviderEvent::SyncCompleted => {
                    tracing::info!("Sync completed, refreshing current chat");
                    self.load_selected_chat_messages();
                    self.refresh_title();
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
                self.capture_new_message_count();
                self.clear_selected_unread();
                self.send_read_receipts().await;
                self.refresh_title();
            }
            Action::PrevChat => {
                self.state.select_prev_chat();
                self.load_selected_chat_messages();
                self.capture_new_message_count();
                self.clear_selected_unread();
                self.send_read_receipts().await;
                self.refresh_title();
            }
            Action::EnterEditing => {
                self.state.enter_editing();
            }
            Action::ExitEditing => {
                self.state.exit_editing();
                self.state.ai_suggestion = None;
                self.state.ai_status = None;
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
            Action::InputKey(key) => {
                self.state.input.input(key);
                self.state.ai_suggestion = None;
                if self.ai_worker.is_some() && self.state.input_mode == InputMode::Editing {
                    self.last_keystroke = Some(Instant::now());
                }
            }
            Action::ClearInput => {
                self.state.input = TextArea::default();
            }
            Action::ScrollUp => {
                self.state.scroll_up();
            }
            Action::ScrollDown => {
                self.state.scroll_down();
            }
            Action::OpenSettings => {
                self.state.open_settings(&self.config, self.state.enter_sends);
            }
            Action::SettingsNext => {
                if let Some(ref mut s) = self.state.settings_state {
                    s.select_next();
                }
            }
            Action::SettingsPrev => {
                if let Some(ref mut s) = self.state.settings_state {
                    s.select_prev();
                }
            }
            Action::SettingsToggle => {
                if let Some(ref mut s) = self.state.settings_state {
                    s.toggle_selected();
                }
            }
            Action::SettingsSave => {
                if let Some(ref settings) = self.state.settings_state {
                    settings.apply_to_config(&mut self.config);
                    if let Err(e) = self.config.save(&self.config_path) {
                        tracing::error!("Failed to save config: {}", e);
                    } else {
                        tracing::info!("Config saved to {}", self.config_path.display());
                    }
                }
                // Save EnterSends to SQLite and apply live (no restart needed)
                if let Some(ref settings) = self.state.settings_state {
                    if let Some(item) = settings.items.iter().find(|i| i.key == SettingsKey::EnterSends) {
                        if let SettingsValue::Bool(v) = item.value {
                            if let Err(e) = self.db.set_preference("enter_sends", if v { "true" } else { "false" }) {
                                tracing::error!("Failed to persist enter_sends preference: {}", e);
                            }
                            self.state.enter_sends = v;
                        }
                    }
                }
                self.state.close_settings();
            }
            Action::SettingsClose => {
                self.state.close_settings();
            }
            Action::RenameChat => {
                if let Some(idx) = self.state.chat_list_state.selected() {
                    if let Some(chat) = self.state.chats.get(idx) {
                        let name = chat
                            .display_name
                            .as_ref()
                            .unwrap_or(&chat.name)
                            .clone();
                        let mut ta = TextArea::from(vec![name]);
                        ta.move_cursor(tui_textarea::CursorMove::End);
                        self.state.input = ta;
                        self.state.input_mode = InputMode::Renaming;
                    }
                }
            }
            Action::ConfirmRename => {
                let new_name = self.state.input.lines()
                    .first()
                    .cloned()
                    .unwrap_or_default();
                self.state.input = tui_textarea::TextArea::default();
                if !new_name.is_empty() {
                    if let Some(idx) = self.state.chat_list_state.selected() {
                        if let Some(chat) = self.state.chats.get_mut(idx) {
                            chat.display_name = Some(new_name.clone());
                            let _ = self.address_book.set_display_name(&chat.id, &new_name);
                        }
                    }
                }
                self.state.input_mode = InputMode::Normal;
            }
            Action::CancelRename => {
                self.state.input = TextArea::default();
                self.state.input_mode = InputMode::Normal;
            }
            Action::OpenChatMenu => {
                self.state.open_chat_menu();
            }
            Action::ChatMenuNext => {
                if let Some(ref mut menu) = self.state.chat_menu_state {
                    menu.select_next();
                }
            }
            Action::ChatMenuPrev => {
                if let Some(ref mut menu) = self.state.chat_menu_state {
                    menu.select_prev();
                }
            }
            Action::ChatMenuConfirm => {
                if let Some(ref menu) = self.state.chat_menu_state {
                    let selected_item = menu.items.get(menu.selected).cloned();
                    let chat_id = menu.chat_id.clone();
                    let new_pinned = !menu.is_pinned;

                    match selected_item {
                        Some(ChatMenuItem::TogglePin) => {
                            // Enforce max 10 pinned chats
                            if new_pinned && self.state.chats.iter().filter(|c| c.is_pinned).count() >= 10 {
                                self.state.close_chat_menu();
                                return;
                            }
                            let _ = self.db.set_chat_pinned(&chat_id, new_pinned);
                            if let Some(chat) = self.state.chats.iter_mut().find(|c| c.id == chat_id) {
                                chat.is_pinned = new_pinned;
                            }
                            self.state.chats.sort_by_key(|c| std::cmp::Reverse(c.is_pinned));
                            let new_idx = self.state.chats.iter().position(|c| c.id == chat_id).unwrap_or(0);
                            self.state.chat_list_state.select(Some(new_idx));
                        }
                        Some(ChatMenuItem::ToggleMute) => {
                            let new_muted = !menu.is_muted;
                            let _ = self.db.set_chat_muted(&chat_id, new_muted);
                            if let Some(chat) = self.state.chats.iter_mut().find(|c| c.id == chat_id) {
                                chat.is_muted = new_muted;
                            }
                        }
                        None => {}
                    }
                }
                self.state.close_chat_menu();
            }
            Action::ChatMenuClose => {
                self.state.close_chat_menu();
            }
            Action::OpenSearch => {
                self.state.search_state = Some(SearchState::new());
                self.state.input_mode = InputMode::Searching;
            }
            Action::SearchClose => {
                self.state.search_state = None;
                self.state.input_mode = InputMode::Normal;
            }
            Action::SearchInput(key) => {
                use crossterm::event::KeyCode;
                if let Some(ref mut ss) = self.state.search_state {
                    match key.code {
                        KeyCode::Backspace => { ss.query.pop(); }
                        KeyCode::Char(c) => { ss.query.push(c); }
                        _ => {}
                    }
                    ss.results = top_fuzzy_matches(&ss.query, &self.state.chats, 5);
                    ss.selected = ss.selected.min(ss.results.len().saturating_sub(1));
                }
            }
            Action::SearchNext => {
                if let Some(ref mut ss) = self.state.search_state {
                    if !ss.results.is_empty() {
                        ss.selected = (ss.selected + 1) % ss.results.len();
                    }
                }
            }
            Action::SearchPrev => {
                if let Some(ref mut ss) = self.state.search_state {
                    if !ss.results.is_empty() {
                        ss.selected = ss.selected.checked_sub(1).unwrap_or(ss.results.len() - 1);
                    }
                }
            }
            Action::SearchConfirm => {
                let chat_idx = self.state.search_state.as_ref()
                    .and_then(|ss| ss.results.get(ss.selected).copied());
                if let Some(idx) = chat_idx {
                    self.state.search_state = None;
                    self.state.enter_editing();
                    self.state.chat_list_state.select(Some(idx));
                    self.load_selected_chat_messages();
                    self.capture_new_message_count();
                    self.clear_selected_unread();
                    self.send_read_receipts().await;
                    self.refresh_title();
                }
            }
            Action::AiSuggestAccept => {
                if let Some(suggestion) = self.state.ai_suggestion.take() {
                    for ch in suggestion.chars() {
                        self.state.input.insert_char(ch);
                    }
                }
            }
            Action::AiSuggestRequest => {
                self.last_keystroke = None;
                if self.state.input_mode == InputMode::Editing {
                    let partial = self.state.input.lines().join("\n");
                    if !partial.is_empty() {
                        if let Some(worker) = self.ai_worker.as_mut() {
                            let messages: Vec<RawMessage> = self.state.messages.iter()
                                .filter_map(|m| {
                                    let text = match &m.content {
                                        crate::core::types::MessageContent::Text(t) => t.clone(),
                                        _ => return None,
                                    };
                                    Some(RawMessage { is_outgoing: m.is_outgoing, text })
                                })
                                .collect();
                            let summary = self.state.selected_chat_id()
                                .and_then(|id| self.db.get_preference(&format!("ai_summary:{}", id)).ok().flatten());
                            self.state.push_ai_log(format!(
                                "[Ctrl+Space] → POST {}/v1/chat/completions | model={} | ctx={} msgs | input={:?}",
                                self.config.ai.base_url, self.config.ai.model, messages.len(),
                                if partial.len() > 40 { &partial[..40] } else { &partial }
                            ));
                            tracing::info!(
                                trigger = "ctrl+space",
                                url = %format!("{}/v1/chat/completions", self.config.ai.base_url),
                                model = %self.config.ai.model,
                                context_msgs = messages.len(),
                                input = %partial,
                                "AI autocomplete request"
                            );
                            worker.request(AiRequest { partial_input: partial, messages, summary });
                        }
                    }
                }
            }
            Action::CopyLastMessage => {
                if let Some(msg) = self.state.messages.last() {
                    let text = msg.content.as_text().to_string();
                    copy_to_clipboard(&text);
                    self.state.copy_status = Some("Copied!".to_string());
                }
            }
            Action::EnterMessageSelect => {
                self.state.enter_message_select();
            }
            Action::MessageSelectPrev => {
                self.state.message_select_prev();
            }
            Action::MessageSelectNext => {
                self.state.message_select_next();
            }
            Action::MessageSelectCopy => {
                if let Some(idx) = self.state.selected_message_idx {
                    if let Some(msg) = self.state.messages.get(idx) {
                        let text = msg.content.as_text().to_string();
                        copy_to_clipboard(&text);
                        self.state.copy_status = Some("Copied!".to_string());
                    }
                }
                self.state.exit_message_select();
            }
            Action::MessageSelectExit => {
                self.state.exit_message_select();
            }
            // Schedule actions — handled in Task 4
            Action::ScheduleMessage
            | Action::ScheduleInput(_)
            | Action::ScheduleConfirm
            | Action::ScheduleCancel
            | Action::OpenScheduleList
            | Action::ScheduleListNext
            | Action::ScheduleListPrev
            | Action::ScheduleListDelete
            | Action::ScheduleListClose => {}
            Action::None => {}
        }
    }

    fn load_selected_chat_messages(&mut self) {
        if let Some(chat_id) = self.state.selected_chat_id().map(|s| s.to_string()) {
            match self.db.get_recent_messages_for_chat(&chat_id, 50) {
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

    async fn send_read_receipts(&mut self) {
        let (chat_id, platform) = match self.state.chat_list_state.selected() {
            Some(idx) => match self.state.chats.get(idx) {
                Some(chat) => (chat.id.clone(), chat.platform),
                None => return,
            },
            None => return,
        };

        // Collect IDs of incoming messages (those are the ones we need to mark as read)
        let msg_ids: Vec<String> = self
            .state
            .messages
            .iter()
            .filter(|m| !m.is_outgoing)
            .map(|m| m.id.clone())
            .collect();

        if msg_ids.is_empty() {
            return;
        }

        if let Some(provider) = self.router.get_provider_mut(platform) {
            if let Err(e) = provider.mark_as_read(&chat_id, msg_ids).await {
                tracing::error!("Failed to send read receipts: {}", e);
            }
        }
    }

    fn capture_new_message_count(&mut self) {
        self.state.new_message_count = self
            .state
            .chat_list_state
            .selected()
            .and_then(|i| self.state.chats.get(i))
            .map(|c| c.unread_count as usize)
            .unwrap_or(0);
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

    fn refresh_title(&self) {
        Self::update_title(self.state.has_unread());
    }

    fn update_title(has_unread: bool) {
        let title = if has_unread { "● zero-drift-chat" } else { "zero-drift-chat" };
        let _ = execute!(io::stdout(), SetTitle(title));
    }

    /// Extract the phone/identifier from a WhatsApp chat_id like `wa-559985213786@s.whatsapp.net`.
    /// Returns the part before `@`, or None for non-WA or group/newsletter chats.
    fn extract_wa_phone(chat_id: &str) -> Option<&str> {
        let raw = chat_id.strip_prefix("wa-")?;
        // Only direct (non-group, non-newsletter) chats
        if raw.contains("@g.us") || raw.contains("@newsletter") || raw.contains("@lid") {
            return None;
        }
        Some(raw.split('@').next().unwrap_or(raw))
    }

    /// Resolve a display name for a chat: address-book display_name first,
    /// then contacts table by phone, then None.
    fn resolve_contact_name(&self, chat_id: &str) -> Option<String> {
        // 1. User-set display name wins
        if let Ok(names) = self.address_book.get_all_display_names() {
            if let Some(name) = names.get(chat_id) {
                return Some(name.clone());
            }
        }
        // 2. Contacts table by phone
        if let Some(phone) = Self::extract_wa_phone(chat_id) {
            if let Ok(Some(name)) = self.address_book.lookup_contact(phone) {
                return Some(name);
            }
        }
        None
    }
}

/// Copy text to the system clipboard using the OSC 52 terminal escape sequence.
/// This works in most modern terminals (kitty, iTerm2, WezTerm, tmux with set-clipboard on, etc.).
fn copy_to_clipboard(text: &str) {
    use std::io::Write;
    let encoded = base64_encode(text.as_bytes());
    // OSC 52 ; c ; <base64> ST
    let osc52 = format!("\x1b]52;c;{}\x07", encoded);
    let _ = std::io::stdout().write_all(osc52.as_bytes());
    let _ = std::io::stdout().flush();
}

/// Minimal base64 encoder (no external crate needed).
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(n & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
