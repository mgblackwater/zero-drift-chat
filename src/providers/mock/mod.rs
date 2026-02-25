use async_trait::async_trait;
use chrono::Utc;
use rand::Rng;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::core::provider::{MessagingProvider, ProviderEvent};
use crate::core::types::*;
use crate::core::Result;

const CHAT_NAMES: &[&str] = &[
    "Alice Johnson",
    "Bob Smith",
    "Team Standup",
    "Eve Martinez",
    "Charlie & Dave",
    "Project Alpha",
    "Mom",
    "Jane Doe",
];

const MOCK_MESSAGES: &[&str] = &[
    "Hey, how's it going?",
    "Did you see the latest update?",
    "Let's catch up later today",
    "Sounds good to me!",
    "Can you review my PR?",
    "Meeting at 3pm, don't forget",
    "Just finished the refactor",
    "Lunch?",
    "I'll send you the details",
    "Thanks for the help!",
    "Running a bit late",
    "Check out this article I found",
    "Good morning everyone!",
    "The build is green now",
    "Happy Friday!",
    "Let me think about it",
    "Agreed, let's go with that approach",
    "Has anyone seen my coffee mug?",
    "Working from home today",
    "Great work on the presentation!",
];

pub struct MockProvider {
    chats: Arc<Mutex<Vec<UnifiedChat>>>,
    messages: Arc<Mutex<Vec<UnifiedMessage>>>,
    task_handle: Option<JoinHandle<()>>,
    tx: Option<mpsc::UnboundedSender<ProviderEvent>>,
    auth_status: AuthStatus,
    chat_count: usize,
    message_interval_secs: u64,
}

impl MockProvider {
    pub fn new(chat_count: usize, message_interval_secs: u64) -> Self {
        Self {
            chats: Arc::new(Mutex::new(Vec::new())),
            messages: Arc::new(Mutex::new(Vec::new())),
            task_handle: None,
            tx: None,
            auth_status: AuthStatus::NotAuthenticated,
            chat_count,
            message_interval_secs,
        }
    }

    fn generate_chats(&self) -> Vec<UnifiedChat> {
        let count = self.chat_count.min(CHAT_NAMES.len());
        (0..count)
            .map(|i| UnifiedChat {
                id: format!("mock-chat-{}", i),
                platform: Platform::Mock,
                name: CHAT_NAMES[i].to_string(),
                display_name: None,
                last_message: None,
                unread_count: 0,
                is_group: i == 2 || i == 4, // "Team Standup" and "Charlie & Dave"
            })
            .collect()
    }

    fn generate_seed_messages(chats: &[UnifiedChat]) -> Vec<UnifiedMessage> {
        let mut rng = rand::thread_rng();
        let mut all_messages = Vec::new();

        for chat in chats {
            let msg_count = rng.gen_range(10..=20);
            let base_time = Utc::now() - chrono::Duration::hours(24);

            for j in 0..msg_count {
                let is_outgoing = rng.gen_bool(0.3);
                let sender = if is_outgoing {
                    "You".to_string()
                } else {
                    chat.name.clone()
                };
                let content_idx = rng.gen_range(0..MOCK_MESSAGES.len());
                let timestamp =
                    base_time + chrono::Duration::minutes(j as i64 * rng.gen_range(5..30));

                all_messages.push(UnifiedMessage {
                    id: Uuid::new_v4().to_string(),
                    chat_id: chat.id.clone(),
                    platform: Platform::Mock,
                    sender,
                    content: MessageContent::Text(MOCK_MESSAGES[content_idx].to_string()),
                    timestamp,
                    status: MessageStatus::Read,
                    is_outgoing,
                });
            }
        }

        all_messages
    }
}

#[async_trait]
impl MessagingProvider for MockProvider {
    async fn start(&mut self, tx: mpsc::UnboundedSender<ProviderEvent>) -> Result<()> {
        self.auth_status = AuthStatus::Authenticated;
        let _ = tx.send(ProviderEvent::AuthStatusChanged(
            Platform::Mock,
            AuthStatus::Authenticated,
        ));

        let chats = self.generate_chats();
        let seed_messages = Self::generate_seed_messages(&chats);

        // Send initial chats
        let _ = tx.send(ProviderEvent::ChatsUpdated(chats.clone()));

        // Send seed messages
        for msg in &seed_messages {
            let _ = tx.send(ProviderEvent::NewMessage(msg.clone()));
        }

        *self.chats.lock().await = chats;
        *self.messages.lock().await = seed_messages;

        // Spawn periodic message generator
        let chats_ref = self.chats.clone();
        let tx_clone = tx.clone();
        let interval = self.message_interval_secs;

        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(
                tokio::time::Duration::from_secs(interval),
            );
            interval_timer.tick().await; // skip first immediate tick

            loop {
                interval_timer.tick().await;
                let chats = chats_ref.lock().await;
                if chats.is_empty() {
                    continue;
                }

                let mut rng = rand::thread_rng();
                let chat_idx = rng.gen_range(0..chats.len());
                let chat = &chats[chat_idx];
                let content_idx = rng.gen_range(0..MOCK_MESSAGES.len());

                let msg = UnifiedMessage {
                    id: Uuid::new_v4().to_string(),
                    chat_id: chat.id.clone(),
                    platform: Platform::Mock,
                    sender: chat.name.clone(),
                    content: MessageContent::Text(MOCK_MESSAGES[content_idx].to_string()),
                    timestamp: Utc::now(),
                    status: MessageStatus::Delivered,
                    is_outgoing: false,
                };

                if tx_clone.send(ProviderEvent::NewMessage(msg)).is_err() {
                    break;
                }
            }
        });

        self.task_handle = Some(handle);
        self.tx = Some(tx);
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
        self.auth_status = AuthStatus::NotAuthenticated;
        Ok(())
    }

    async fn send_message(&self, chat_id: &str, content: MessageContent) -> Result<UnifiedMessage> {
        let msg = UnifiedMessage {
            id: Uuid::new_v4().to_string(),
            chat_id: chat_id.to_string(),
            platform: Platform::Mock,
            sender: "You".to_string(),
            content,
            timestamp: Utc::now(),
            status: MessageStatus::Sent,
            is_outgoing: true,
        };

        self.messages.lock().await.push(msg.clone());

        if let Some(tx) = &self.tx {
            let _ = tx.send(ProviderEvent::NewMessage(msg.clone()));
        }

        Ok(msg)
    }

    async fn get_chats(&self) -> Result<Vec<UnifiedChat>> {
        Ok(self.chats.lock().await.clone())
    }

    async fn get_messages(&self, chat_id: &str) -> Result<Vec<UnifiedMessage>> {
        let messages = self.messages.lock().await;
        Ok(messages
            .iter()
            .filter(|m| m.chat_id == chat_id)
            .cloned()
            .collect())
    }

    fn name(&self) -> &str {
        "Mock"
    }

    fn platform(&self) -> Platform {
        Platform::Mock
    }

    fn auth_status(&self) -> AuthStatus {
        self.auth_status
    }
}
