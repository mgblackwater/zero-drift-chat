use crate::core::types::*;

// --- PeerCache (populated during get_chats, reused for send/get_messages) ---

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Maps our `tg-{peer_id}` chat IDs to grammers PeerRef handles.
/// Populated during `get_chats()`; reused in `send_message()` and `get_messages()`.
/// `PeerRef` is the correct grammers type to store — it implements `Into<PeerRef>`
/// which all client methods accept via `C: Into<PeerRef>`.
#[derive(Clone, Default)]
pub struct PeerCache {
    inner: Arc<Mutex<HashMap<String, grammers_session::types::PeerRef>>>,
}

impl PeerCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, chat_id: &str, peer: grammers_session::types::PeerRef) {
        self.inner.lock().unwrap().insert(chat_id.to_string(), peer);
    }

    pub fn get(&self, chat_id: &str) -> Option<grammers_session::types::PeerRef> {
        self.inner.lock().unwrap().get(chat_id).copied()
    }
}

/// Encode a Telegram peer id (i64) to our chat_id string format.
pub fn peer_id_to_chat_id(peer_id: i64) -> String {
    format!("tg-{}", peer_id)
}

/// Decode our chat_id string back to a peer id (i64).
/// Returns None if the format is wrong.
#[allow(dead_code)]
pub fn chat_id_to_peer_id(chat_id: &str) -> Option<i64> {
    chat_id.strip_prefix("tg-")?.parse::<i64>().ok()
}

/// Convert a grammers `Message` to `UnifiedMessage`.
/// Returns `None` if the message has no usable text content (e.g., service messages we skip).
pub fn grammers_message_to_unified(
    msg: &grammers_client::message::Message,
    chat_id: &str,
) -> Option<UnifiedMessage> {
    // Text content — media becomes "[Media]" placeholder (v1)
    let text = if msg.text().is_empty() {
        "[Media]".to_string()
    } else {
        msg.text().to_string()
    };

    let sender = msg
        .sender()
        .and_then(|s| s.name())
        .map(|n| n.to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    Some(UnifiedMessage {
        id: msg.id().to_string(),
        chat_id: chat_id.to_string(),
        platform: Platform::Telegram,
        sender,
        content: MessageContent::Text(text),
        // msg.date() already returns DateTime<Utc>
        timestamp: msg.date(),
        status: MessageStatus::Sent,
        is_outgoing: msg.outgoing(),
    })
}

// NOTE: grammers types cannot be constructed in unit tests — their
// constructors are private. The chat-id encode/decode round-trip tests
// above are the meaningful unit coverage. Integration behaviour is
// verified manually via the live provider.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_id_round_trip_positive() {
        let id: i64 = 123456789;
        let chat_id = peer_id_to_chat_id(id);
        assert_eq!(chat_id, "tg-123456789");
        assert_eq!(chat_id_to_peer_id(&chat_id), Some(id));
    }

    #[test]
    fn test_chat_id_round_trip_negative() {
        // Telegram uses negative IDs for groups/channels
        let id: i64 = -1001234567890;
        let chat_id = peer_id_to_chat_id(id);
        assert_eq!(chat_id, "tg--1001234567890");
        assert_eq!(chat_id_to_peer_id(&chat_id), Some(id));
    }

    #[test]
    fn test_chat_id_invalid() {
        assert_eq!(chat_id_to_peer_id("wa-12345"), None);
        assert_eq!(chat_id_to_peer_id("tg-notanumber"), None);
        assert_eq!(chat_id_to_peer_id(""), None);
    }
}
