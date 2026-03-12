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
    Image {
        url: String,
        caption: Option<String>,
    },
    File {
        url: String,
        filename: String,
    },
    System(String),
}

impl MessageContent {
    pub fn as_text(&self) -> &str {
        match self {
            MessageContent::Text(t) => t,
            MessageContent::Image { caption, .. } => caption.as_deref().unwrap_or("[Image]"),
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

#[allow(dead_code)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ChatKind {
    #[default]
    Chat, // 1:1 DM with a human
    Group,      // WA @g.us group or TG Group/Supergroup
    Channel,    // TG broadcast channel
    Newsletter, // WA @newsletter
    Bot,        // TG bot user
}

impl ChatKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChatKind::Chat => "chat",
            ChatKind::Group => "group",
            ChatKind::Channel => "channel",
            ChatKind::Newsletter => "newsletter",
            ChatKind::Bot => "bot",
        }
    }

    pub fn from_str(s: &str) -> ChatKind {
        match s {
            "group" => ChatKind::Group,
            "channel" => ChatKind::Channel,
            "newsletter" => ChatKind::Newsletter,
            "bot" => ChatKind::Bot,
            _ => ChatKind::Chat, // default / unknown values
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedChat {
    pub id: String,
    pub platform: Platform,
    pub name: String,
    pub display_name: Option<String>,
    pub last_message: Option<String>,
    pub unread_count: u32,
    pub kind: ChatKind,
    pub is_pinned: bool,
    pub is_muted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_kind_roundtrip() {
        let variants = [
            ChatKind::Chat,
            ChatKind::Group,
            ChatKind::Channel,
            ChatKind::Newsletter,
            ChatKind::Bot,
        ];
        for kind in &variants {
            assert_eq!(ChatKind::from_str(kind.as_str()), *kind);
        }
    }

    #[test]
    fn chat_kind_from_str_unknown_defaults_to_chat() {
        assert_eq!(ChatKind::from_str("unknown_value"), ChatKind::Chat);
        assert_eq!(ChatKind::from_str(""), ChatKind::Chat);
    }
}
