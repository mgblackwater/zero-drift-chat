use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    WhatsApp,
    Telegram,
    Slack,
    Mock,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::WhatsApp => write!(f, "WA"),
            Platform::Telegram => write!(f, "TG"),
            Platform::Slack => write!(f, "SL"),
            Platform::Mock => write!(f, "Mock"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageStatus {
    Sending,
    Sent,
    Delivered,
    Read,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    Image { url: String, caption: Option<String> },
    File { url: String, filename: String },
    System(String),
}

impl MessageContent {
    pub fn as_text(&self) -> &str {
        match self {
            MessageContent::Text(t) => t,
            MessageContent::Image { caption, .. } => {
                caption.as_deref().unwrap_or("[Image]")
            }
            MessageContent::File { filename, .. } => filename,
            MessageContent::System(t) => t,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthStatus {
    NotAuthenticated,
    Authenticating,
    Authenticated,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub platform: Platform,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMessage {
    pub id: String,
    pub chat_id: String,
    pub platform: Platform,
    pub sender: String,
    pub content: MessageContent,
    pub timestamp: DateTime<Utc>,
    pub status: MessageStatus,
    pub is_outgoing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedChat {
    pub id: String,
    pub platform: Platform,
    pub name: String,
    pub last_message: Option<String>,
    pub unread_count: u32,
    pub is_group: bool,
}
