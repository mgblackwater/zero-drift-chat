use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::{
    event::{DisableBracketedPaste, EnableBracketedPaste, KeyCode},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::ai::context::RawMessage;
use crate::ai::providers::anthropic::AnthropicClient;
use crate::ai::providers::gemini::GeminiClient;
use crate::ai::providers::openai::OpenAiClient;
use crate::ai::worker::{AiRequest, AiWorker};
use crate::config::AppConfig;
use crate::core::provider::ProviderEvent;
use crate::core::types::{AuthStatus, MessageContent, Platform};
use crate::core::MessageRouter;
use crate::providers::mock::MockProvider;
use crate::providers::whatsapp::WhatsAppProvider;
use crate::storage::{AddressBook, Database, ScheduledMessage};
use tui_textarea::TextArea;

use crate::tui;
use crate::tui::app_state::{
    AppState, ChatMenuItem, InputMode, ScheduleListState, SchedulePromptState, SearchState,
    SettingsKey, SettingsValue, TypingInfo,
};
use crate::tui::event::{AppEvent, EventHandler};
use crate::tui::keybindings::{map_key, Action};
use crate::tui::render;
use crate::tui::search::top_fuzzy_matches;
use crate::tui::time_parse::{format_local_time, parse_schedule_time};

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
    schedule_status_ticks: u8,
    tick_count: u64,
    telegram_auth_tx:
        Option<tokio::sync::mpsc::UnboundedSender<crate::providers::telegram::AuthInput>>,
    event_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
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
            let provider: Box<dyn crate::ai::providers::AiProvider> =
                match config.ai.provider.as_str() {
                    "anthropic" => Box::new(AnthropicClient::new(config.ai.api_key.clone())),
                    "gemini" => Box::new(GeminiClient::new(config.ai.api_key.clone())),
                    _ => Box::new(OpenAiClient::new(
                        config.ai.base_url.clone(),
                        config.ai.api_key.clone(),
                    )),
                };
            tracing::info!("AI worker created — autocomplete enabled");
            Some(AiWorker::new(provider, config.ai.clone(), event_tx.clone()))
        } else {
            tracing::info!("AI worker NOT created — ai.enabled = false in config");
            None
        };

        let (db_summary_tx, db_summary_rx) =
            tokio::sync::mpsc::unbounded_channel::<(String, String)>();

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
            schedule_status_ticks: 0,
            tick_count: 0,
            telegram_auth_tx: None,
            event_tx,
        }
    }

    pub async fn run(&mut self, mut events: EventHandler) -> anyhow::Result<()> {
        // Load enter_sends preference from DB (default true)
        self.state.enter_sends = self
            .db
            .get_preference("enter_sends")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(true);
        // Load show_activity_graph preference from DB (default true)
        self.state.show_activity_graph = self
            .db
            .get_preference("show_activity_graph")
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
            let session_path = format!("{}/whatsapp-session.db", self.config.general.data_dir);
            let lid_mappings = self.db.load_lid_mappings().unwrap_or_default();
            let wa = WhatsAppProvider::new_with_lid_mappings(session_path, lid_mappings);
            self.router.register_provider(Box::new(wa));
        }

        if self.config.telegram.enabled {
            let api_id = self.config.telegram.api_id;
            let api_hash = self.config.telegram.api_hash.clone();

            if api_id == 0 || api_hash.is_empty() {
                tracing::error!(
                    "Telegram enabled but api_id or api_hash not configured — skipping"
                );
            } else {
                let session_path = format!("{}/telegram-session.db", self.config.general.data_dir);
                let tg = crate::providers::telegram::TelegramProvider::new(
                    api_id,
                    api_hash,
                    session_path,
                );
                // Stash the auth_tx so we can forward TUI input to the provider's auth task
                self.telegram_auth_tx = Some(tg.auth_tx.clone());
                self.router.register_provider(Box::new(tg));
            }
        }

        // Start all providers
        self.router.start_all().await?;
        tokio::task::spawn_blocking(crate::tui::media::cleanup_temp_images);

        // Track which providers are enabled for status bar
        self.state.mock_enabled = self.config.mock_provider.enabled;

        // Load persisted chats from DB, filtering out disabled providers
        if let Ok(chats) = self.db.get_all_chats() {
            let mut chats: Vec<_> = chats
                .into_iter()
                .filter(|c| match c.platform {
                    Platform::Mock => self.config.mock_provider.enabled,
                    Platform::WhatsApp => self.config.whatsapp.enabled,
                    Platform::Telegram => self.config.telegram.enabled,
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

        // Populate activity cache before first render
        self.refresh_activity_cache();
        self.state.activity_last_refresh_tick = 0;

        // Send any overdue scheduled messages
        self.check_scheduled_messages().await;

        // Set initial terminal title
        self.refresh_title();

        // Set up terminal
        enable_raw_mode()?;
        // EventStream::new() requires raw mode to be active — start the task now.
        events.start();
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
                                        let messages: Vec<RawMessage> = self
                                            .state
                                            .messages
                                            .iter()
                                            .filter_map(|m| {
                                                let text = match &m.content {
                                                    crate::core::types::MessageContent::Text(t) => {
                                                        t.clone()
                                                    }
                                                    _ => return None,
                                                };
                                                Some(RawMessage {
                                                    is_outgoing: m.is_outgoing,
                                                    text,
                                                })
                                            })
                                            .collect();
                                        let summary =
                                            self.state.selected_chat_id().and_then(|id| {
                                                self.db
                                                    .get_preference(&format!("ai_summary:{}", id))
                                                    .ok()
                                                    .flatten()
                                            });
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
                                        worker.request(AiRequest {
                                            partial_input: partial,
                                            messages,
                                            summary,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    self.check_scheduled_messages().await;
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
                Some(AppEvent::MediaError(e)) => {
                    tracing::error!(error = %e, "Media open error");
                    self.state.copy_status = Some(e);
                }
                Some(AppEvent::Quit) | None => {
                    break;
                }
            }
        }

        // Cleanup
        if let Some(worker) = &mut self.ai_worker {
            worker.shutdown();
        }
        self.router.stop_all().await?;
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            DisableBracketedPaste,
            LeaveAlternateScreen
        )?;
        Self::update_title(false); // restore clean title on exit
        terminal.show_cursor()?;

        Ok(())
    }

    fn refresh_activity_cache(&mut self) {
        let chat_ids: Vec<String> = self.state.chats.iter().map(|c| c.id.clone()).collect();
        let id_refs: Vec<&str> = chat_ids.iter().map(|s| s.as_str()).collect();
        let cache = crate::storage::activity::query_activity_24h(&self.db, &id_refs);
        self.state.activity_cache = cache;
    }

    fn handle_tick(&mut self) {
        // Clear transient copy status after one tick so it disappears quickly
        self.state.copy_status = None;

        if self.state.schedule_status.is_some() {
            self.schedule_status_ticks += 1;
            if self.schedule_status_ticks >= 8 {
                self.state.schedule_status = None;
                self.schedule_status_ticks = 0;
            }
        } else {
            self.schedule_status_ticks = 0;
        }

        // Advance tick counter and drive typing indicator animation
        self.tick_count += 1;

        // Refresh activity cache every 1200 ticks (~5 minutes at 250ms/tick)
        if self
            .tick_count
            .saturating_sub(self.state.activity_last_refresh_tick)
            >= 1200
        {
            self.state.activity_last_refresh_tick = self.tick_count;
            self.refresh_activity_cache();
        }

        let now = std::time::Instant::now();
        self.state.typing_states.retain(|_, v| v.expires_at > now);
        if self.tick_count.is_multiple_of(2) {
            self.state.blink_phase = (self.state.blink_phase + 1) % 3;
        }

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

                    // Increment activity cache for the current hour bucket (slot 23).
                    self.state
                        .activity_cache
                        .entry(msg.chat_id.clone())
                        .or_insert([0u32; 24])[23] += 1;

                    // Store push_name as contact so we can name phone-number chats
                    if !msg.is_outgoing && msg.sender != "You" && !msg.sender.is_empty() {
                        if let Some(phone) = Self::extract_wa_phone(&msg.chat_id) {
                            let _ = self.address_book.upsert_contact(phone, &msg.sender);
                            // Apply to the in-memory chat if it still shows a phone number
                            if let Some(chat) =
                                self.state.chats.iter_mut().find(|c| c.id == msg.chat_id)
                            {
                                if chat.display_name.is_none() {
                                    let is_numeric =
                                        chat.name.chars().all(|c| c.is_ascii_digit() || c == '+');
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
                        if let Some(chat) =
                            self.state.chats.iter_mut().find(|c| c.id == msg.chat_id)
                        {
                            chat.unread_count += 1;
                            let _ = self.db.update_unread_count(&chat.id, chat.unread_count);
                            self.refresh_title();
                        }
                    }

                    // Update last message preview
                    if let Some(chat) = self.state.chats.iter_mut().find(|c| c.id == msg.chat_id) {
                        chat.last_message = Some(preview);
                    }

                    // Add to current view if it's the selected chat
                    if is_current_chat {
                        self.state.messages.push(msg.clone());
                        self.state.scroll_offset = 0; // auto-scroll to bottom

                        if let Some(worker) = &mut self.ai_worker {
                            if let Some(chat_id) = self.state.selected_chat_id() {
                                let messages: Vec<crate::ai::context::RawMessage> = self
                                    .state
                                    .messages
                                    .iter()
                                    .filter_map(|m| {
                                        let text = match &m.content {
                                            crate::core::types::MessageContent::Text(t) => {
                                                t.clone()
                                            }
                                            _ => return None,
                                        };
                                        Some(crate::ai::context::RawMessage {
                                            is_outgoing: m.is_outgoing,
                                            text,
                                        })
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
                            self.state
                                .chats
                                .iter()
                                .rposition(|c| c.is_pinned)
                                .map(|p| p + 1)
                                .unwrap_or(0)
                        };
                        if pos != insert_pos {
                            self.state.chats.insert(insert_pos, chat);
                            if let Some(id) = selected_id {
                                if let Some(new_pos) =
                                    self.state.chats.iter().position(|c| c.id == id)
                                {
                                    self.state.chat_list_state.select(Some(new_pos));
                                }
                            }
                        } else {
                            self.state.chats.insert(pos, chat);
                        }
                    }
                }
                ProviderEvent::MessageUpdated(msg) => {
                    // Upsert to DB — INSERT OR REPLACE keeps the row current.
                    if let Err(e) = self.db.insert_message(&msg) {
                        tracing::debug!("MessageUpdated db upsert: {}", e);
                    }

                    // Update last_message preview on the chat
                    let preview = msg.content.as_text().to_string();
                    if let Some(chat) = self.state.chats.iter_mut().find(|c| c.id == msg.chat_id) {
                        chat.last_message = Some(preview.clone());
                    }
                    let _ = self.db.update_last_message(&msg.chat_id, &preview);

                    // Update message in-place if this chat is currently open
                    let is_current_chat = self
                        .state
                        .selected_chat_id()
                        .map(|id| id == msg.chat_id)
                        .unwrap_or(false);

                    if is_current_chat {
                        if let Some(pos) = self
                            .state
                            .messages
                            .iter()
                            .position(|m| m.id == msg.id && m.chat_id == msg.chat_id)
                        {
                            tracing::debug!(
                                msg_id = %msg.id,
                                "MessageUpdated: replacing message at pos {}",
                                pos
                            );
                            self.state.messages[pos] = msg;
                        } else {
                            // Edit arrived before history was loaded — push as new.
                            tracing::debug!(
                                msg_id = %msg.id,
                                "MessageUpdated: message not in current view, appending"
                            );
                            self.state.messages.push(msg);
                            self.state.scroll_offset = 0;
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
                        if let Some(existing) =
                            self.state.chats.iter_mut().find(|c| c.id == chat.id)
                        {
                            if chat.last_message.is_some() {
                                existing.last_message = chat.last_message;
                            }
                            // Don't touch display_name — it's user-set
                            // Only update provider name if it looks better
                            let existing_is_numeric = existing
                                .name
                                .chars()
                                .all(|c| c.is_ascii_digit() || c == '+');
                            let new_is_numeric =
                                chat.name.chars().all(|c| c.is_ascii_digit() || c == '+');
                            if !new_is_numeric || existing_is_numeric {
                                existing.name = chat.name;
                            }
                        } else {
                            // Apply address-book / contact name to newly discovered chats
                            let mut new_chat = chat;
                            if let Some(name) = self.resolve_contact_name(&new_chat.id) {
                                new_chat.display_name = Some(name);
                            } else if let Some(phone) = Self::extract_wa_phone(&new_chat.id) {
                                if let Ok(Some(contact_name)) =
                                    self.address_book.lookup_contact(phone)
                                {
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
                    if let Some(msg) = self.state.messages.iter_mut().find(|m| m.id == message_id) {
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
                    if platform == Platform::Telegram {
                        match status {
                            AuthStatus::Authenticated => {
                                self.state.close_telegram_auth();
                            }
                            AuthStatus::Failed => {
                                self.state.close_telegram_auth();
                                tracing::error!("Telegram authentication failed");
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
                    if self
                        .state
                        .selected_chat_id()
                        .map(|id| id == chat_id)
                        .unwrap_or(false)
                    {
                        self.state.new_message_count = 0;
                    }
                    self.refresh_title();
                }
                ProviderEvent::SyncCompleted => {
                    tracing::info!("Sync completed, refreshing current chat");
                    self.load_selected_chat_messages();
                    self.refresh_title();
                }
                ProviderEvent::AuthPhonePrompt(platform, error_hint) => {
                    if platform == Platform::Telegram {
                        self.state.open_telegram_auth(
                            crate::tui::app_state::TelegramAuthStage::Phone,
                            error_hint,
                        );
                    }
                }
                ProviderEvent::AuthOtpPrompt(platform, error_hint) => {
                    if platform == Platform::Telegram {
                        self.state.open_telegram_auth(
                            crate::tui::app_state::TelegramAuthStage::Otp,
                            error_hint,
                        );
                    }
                }
                ProviderEvent::AuthPasswordPrompt(platform, error_hint) => {
                    if platform == Platform::Telegram {
                        self.state.open_telegram_auth(
                            crate::tui::app_state::TelegramAuthStage::Password,
                            error_hint,
                        );
                    }
                }
                ProviderEvent::LidPnMappingDiscovered { lid, pn } => {
                    if let Err(e) = self.db.save_lid_mapping(&lid, &pn) {
                        tracing::error!("Failed to save LID mapping: {}", e);
                    }
                    // Remove stale @lid chat from DB and in-memory state
                    let lid_chat_id = format!("wa-{}", lid);
                    if let Err(e) = self.db.delete_lid_chat(&lid_chat_id) {
                        tracing::error!("Failed to delete stale @lid chat: {}", e);
                    }
                    self.state.chats.retain(|c| c.id != lid_chat_id);
                    tracing::info!(
                        "LID→PN mapping recorded: {} → {}; removed stale chat {}",
                        lid,
                        pn,
                        lid_chat_id
                    );
                }
                ProviderEvent::Typing { chat_id, user_name } => {
                    self.state.typing_states.insert(
                        chat_id,
                        TypingInfo {
                            user_name,
                            expires_at: std::time::Instant::now()
                                + std::time::Duration::from_secs(5),
                        },
                    );
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
                self.state.open_settings(
                    &self.config,
                    self.state.enter_sends,
                    self.state.show_activity_graph,
                );
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
                    if let Some(item) = settings
                        .items
                        .iter()
                        .find(|i| i.key == SettingsKey::EnterSends)
                    {
                        if let SettingsValue::Bool(v) = item.value {
                            if let Err(e) = self
                                .db
                                .set_preference("enter_sends", if v { "true" } else { "false" })
                            {
                                tracing::error!("Failed to persist enter_sends preference: {}", e);
                            }
                            self.state.enter_sends = v;
                        }
                    }
                    if let Some(item) = settings
                        .items
                        .iter()
                        .find(|i| i.key == SettingsKey::ActivityGraph)
                    {
                        if let SettingsValue::Bool(v) = item.value {
                            if let Err(e) = self.db.set_preference(
                                "show_activity_graph",
                                if v { "true" } else { "false" },
                            ) {
                                tracing::error!(
                                    "Failed to persist show_activity_graph preference: {}",
                                    e
                                );
                            }
                            self.state.show_activity_graph = v;
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
                        let name = chat.display_name.as_ref().unwrap_or(&chat.name).clone();
                        let mut ta = TextArea::from(vec![name]);
                        ta.move_cursor(tui_textarea::CursorMove::End);
                        self.state.input = ta;
                        self.state.input_mode = InputMode::Renaming;
                    }
                }
            }
            Action::ConfirmRename => {
                let new_name = self
                    .state
                    .input
                    .lines()
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
                            if new_pinned
                                && self.state.chats.iter().filter(|c| c.is_pinned).count() >= 10
                            {
                                self.state.close_chat_menu();
                                return;
                            }
                            let _ = self.db.set_chat_pinned(&chat_id, new_pinned);
                            if let Some(chat) =
                                self.state.chats.iter_mut().find(|c| c.id == chat_id)
                            {
                                chat.is_pinned = new_pinned;
                            }
                            self.state
                                .chats
                                .sort_by_key(|c| std::cmp::Reverse(c.is_pinned));
                            let new_idx = self
                                .state
                                .chats
                                .iter()
                                .position(|c| c.id == chat_id)
                                .unwrap_or(0);
                            self.state.chat_list_state.select(Some(new_idx));
                        }
                        Some(ChatMenuItem::ToggleMute) => {
                            let new_muted = !menu.is_muted;
                            let _ = self.db.set_chat_muted(&chat_id, new_muted);
                            if let Some(chat) =
                                self.state.chats.iter_mut().find(|c| c.id == chat_id)
                            {
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
                        KeyCode::Backspace => {
                            ss.query.pop();
                        }
                        KeyCode::Char(c) => {
                            ss.query.push(c);
                        }
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
                let chat_idx = self
                    .state
                    .search_state
                    .as_ref()
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
                            let messages: Vec<RawMessage> = self
                                .state
                                .messages
                                .iter()
                                .filter_map(|m| {
                                    let text = match &m.content {
                                        crate::core::types::MessageContent::Text(t) => t.clone(),
                                        _ => return None,
                                    };
                                    Some(RawMessage {
                                        is_outgoing: m.is_outgoing,
                                        text,
                                    })
                                })
                                .collect();
                            let summary = self.state.selected_chat_id().and_then(|id| {
                                self.db
                                    .get_preference(&format!("ai_summary:{}", id))
                                    .ok()
                                    .flatten()
                            });
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
                            worker.request(AiRequest {
                                partial_input: partial,
                                messages,
                                summary,
                            });
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
            Action::OpenMedia => {
                if let Some(idx) = self.state.selected_message_idx {
                    if let Some(msg) = self.state.messages.get(idx) {
                        match &msg.content {
                            MessageContent::Image {
                                url,
                                decrypt_params,
                                ..
                            } => {
                                let url = url.clone();
                                let _url = &url; // reserved for non-E2EE CDN path
                                let platform = msg.platform;

                                if let Some(params) = decrypt_params.clone() {
                                    // E2EE path: download + decrypt via provider, then open
                                    if let Some(provider) = self.router.get_provider_mut(platform) {
                                        match provider.download_media(&params).await {
                                            Ok(bytes) => {
                                                let cache_key = params.direct_path.clone();
                                                let mime = params.mime_type.clone();
                                                let err_tx = self.event_tx.clone();
                                                tokio::spawn(async move {
                                                    if let Err(e) =
                                                        crate::tui::media::open_image_from_bytes(
                                                            bytes,
                                                            &cache_key,
                                                            mime.as_deref(),
                                                        )
                                                        .await
                                                    {
                                                        tracing::error!(
                                                            "Failed to open image: {}",
                                                            e
                                                        );
                                                        let _ = err_tx.send(AppEvent::MediaError(
                                                            format!("Failed to open image: {}", e),
                                                        ));
                                                    }
                                                });
                                                self.state.copy_status =
                                                    Some("Opening image...".to_string());
                                            }
                                            Err(e) => {
                                                tracing::error!("Failed to download media: {}", e);
                                                self.state.copy_status = Some(format!(
                                                    "Failed to download image: {}",
                                                    e
                                                ));
                                            }
                                        }
                                    } else {
                                        tracing::error!(
                                            "No provider found for platform {:?}",
                                            platform
                                        );
                                        self.state.copy_status =
                                            Some("No provider for this message".to_string());
                                    }
                                } else {
                                    // No decrypt params — the image is E2EE encrypted on the CDN
                                    // but the decryption keys were not captured when this message
                                    // was received (e.g. older messages synced from history).
                                    self.state.copy_status = Some(
                                        "Image cannot be opened — decryption keys unavailable (message received before key capture was supported)".to_string(),
                                    );
                                }
                            }
                            MessageContent::Text(t) if t.contains("[Image]") => {
                                // Old history-sync messages stored as Text("[Image]") before
                                // image viewing support was added — no download URL available.
                                self.state.copy_status = Some(
                                    "Image not available — received before image viewing was supported".to_string(),
                                );
                            }
                            _ => {}
                        }
                    }
                }
                self.state.exit_message_select();
            }
            // Schedule actions
            Action::ScheduleMessage => {
                let input = self.state.take_input();
                if !input.is_empty() {
                    if let Some(chat_id) = self.state.selected_chat_id().map(|s| s.to_string()) {
                        let platform = self
                            .state
                            .chats
                            .iter()
                            .find(|c| c.id == chat_id)
                            .map(|c| c.platform)
                            .unwrap_or(Platform::Mock);
                        self.state.schedule_prompt_state =
                            Some(SchedulePromptState::new(input, chat_id, platform));
                        self.state.input_mode = InputMode::SchedulePrompt;
                    }
                }
            }
            Action::ScheduleInput(key) => {
                if let Some(ref mut sp) = self.state.schedule_prompt_state {
                    match key.code {
                        KeyCode::Backspace => {
                            sp.query.pop();
                        }
                        KeyCode::Char(c) => {
                            sp.query.push(c);
                        }
                        _ => {}
                    }
                }
            }
            Action::ScheduleConfirm => {
                if let Some(sp) = self.state.schedule_prompt_state.take() {
                    if let Some(send_at) = parse_schedule_time(&sp.query) {
                        let msg = ScheduledMessage {
                            id: uuid::Uuid::new_v4().to_string(),
                            chat_id: sp.chat_id,
                            platform: sp.platform,
                            content: MessageContent::Text(sp.message_text),
                            send_at,
                            status: "pending".to_string(),
                            created_at: chrono::Utc::now(),
                        };
                        if let Err(e) = self.db.insert_scheduled_message(&msg) {
                            tracing::error!("Failed to schedule message: {}", e);
                        } else {
                            self.state.schedule_status =
                                Some(format!("Scheduled for {}", format_local_time(&send_at)));
                            self.schedule_status_ticks = 0;
                            tracing::info!("Scheduled message for {}", format_local_time(&send_at));
                        }
                    } else {
                        self.state.schedule_status = Some(
                            "Could not parse time — try 'tomorrow 9am' or 'Mar 15 14:30'"
                                .to_string(),
                        );
                        self.schedule_status_ticks = 0;
                    }
                    self.state.input_mode = InputMode::Editing;
                }
            }
            Action::ScheduleCancel => {
                if let Some(sp) = self.state.schedule_prompt_state.take() {
                    // Put the message text back into the input
                    self.state.input = TextArea::default();
                    for ch in sp.message_text.chars() {
                        self.state.input.insert_char(ch);
                    }
                }
                self.state.input_mode = InputMode::Editing;
            }
            Action::OpenScheduleList => match self.db.get_pending_scheduled_messages() {
                Ok(messages) => {
                    self.state.schedule_list_state = Some(ScheduleListState::new(messages));
                    self.state.input_mode = InputMode::ScheduleList;
                }
                Err(e) => {
                    tracing::error!("Failed to load scheduled messages: {}", e);
                }
            },
            Action::ScheduleListNext => {
                if let Some(ref mut sl) = self.state.schedule_list_state {
                    sl.select_next();
                }
            }
            Action::ScheduleListPrev => {
                if let Some(ref mut sl) = self.state.schedule_list_state {
                    sl.select_prev();
                }
            }
            Action::ScheduleListDelete => {
                if let Some(ref mut sl) = self.state.schedule_list_state {
                    if let Some(msg) = sl.messages.get(sl.selected) {
                        match self.db.update_scheduled_status(&msg.id, "cancelled") {
                            Ok(_) => {
                                sl.messages.remove(sl.selected);
                                if sl.selected > 0 && sl.selected >= sl.messages.len() {
                                    sl.selected = sl.messages.len().saturating_sub(1);
                                }
                                self.state.schedule_status = Some("Schedule cancelled".to_string());
                                self.schedule_status_ticks = 0;
                            }
                            Err(e) => {
                                tracing::error!("Failed to cancel scheduled message: {}", e);
                            }
                        }
                    }
                    if sl.messages.is_empty() {
                        self.state.schedule_list_state = None;
                        self.state.input_mode = InputMode::Normal;
                    }
                }
            }
            Action::ScheduleListClose => {
                self.state.schedule_list_state = None;
                self.state.input_mode = InputMode::Normal;
            }
            Action::TelegramAuthChar(c) => {
                if let Some(ref mut auth) = self.state.telegram_auth_state {
                    if !c.is_control() {
                        auth.input.push(c);
                    }
                }
            }
            Action::TelegramAuthBackspace => {
                if let Some(ref mut auth) = self.state.telegram_auth_state {
                    auth.input.pop();
                }
            }
            Action::TelegramAuthSubmit => {
                let value = self.state.take_telegram_auth_input();
                if !value.trim().is_empty() {
                    if let (Some(ref tx), Some(ref auth)) =
                        (&self.telegram_auth_tx, &self.state.telegram_auth_state)
                    {
                        use crate::providers::telegram::AuthInput;
                        use crate::tui::app_state::TelegramAuthStage;
                        let auth_input = match auth.stage {
                            TelegramAuthStage::Phone => AuthInput::Phone(value),
                            TelegramAuthStage::Otp => AuthInput::Otp(value),
                            TelegramAuthStage::Password => AuthInput::Password(value),
                        };
                        if tx.send(auth_input).is_err() {
                            tracing::warn!(
                                "Telegram auth_tx send failed — receiver may have dropped"
                            );
                        }
                    }
                    // Close overlay; it will re-open if auth needs another step
                    self.state.close_telegram_auth();
                }
            }
            Action::TelegramAuthCancel => {
                self.state.close_telegram_auth();
                tracing::info!("Telegram auth cancelled by user");
            }
            Action::None => {}
        }
    }

    async fn check_scheduled_messages(&mut self) {
        let due = match self.db.get_due_scheduled_messages() {
            Ok(msgs) => msgs,
            Err(e) => {
                tracing::error!("Failed to query scheduled messages: {}", e);
                return;
            }
        };

        let mut sent_count = 0;
        for msg in due {
            let chat_id = msg.chat_id.clone();
            let content = msg.content.clone();
            if let Some(provider) = self.router.get_provider_mut(msg.platform) {
                match provider.send_message(&chat_id, content).await {
                    Ok(_) => {
                        let _ = self.db.update_scheduled_status(&msg.id, "sent");
                        sent_count += 1;
                        tracing::info!("Sent scheduled message {} to {}", msg.id, chat_id);
                    }
                    Err(e) => {
                        tracing::error!("Failed to send scheduled message {}: {}", msg.id, e);
                        // Leave as pending — will retry next tick
                    }
                }
            }
        }
        if sent_count > 0 {
            self.state.schedule_status = Some(format!(
                "Sent {} scheduled message{}",
                sent_count,
                if sent_count == 1 { "" } else { "s" }
            ));
            self.schedule_status_ticks = 0;
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
        let title = if has_unread {
            "● zero-drift-chat"
        } else {
            "zero-drift-chat"
        };
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
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
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
