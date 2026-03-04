use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use whatsapp_rust::proto_helpers::MessageExt;
use whatsapp_rust::waproto::whatsapp as wa;
use whatsapp_rust::Jid;

use crate::core::types::*;

/// Cache mapping LID JID strings to their PN (phone number) JID equivalents.
/// WhatsApp uses two JID formats for the same person:
/// - PN: `559985213786@s.whatsapp.net`
/// - LID: `39492358562039@lid`
#[derive(Clone, Default)]
pub struct JidCache {
    lid_to_pn: Arc<Mutex<HashMap<String, String>>>,
}

impl JidCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a mapping between two JIDs (auto-detects LID vs PN).
    pub fn record_mapping(&self, jid_a: &Jid, jid_b: &Jid) {
        let a = jid_a.to_string();
        let b = jid_b.to_string();
        let mut map = self.lid_to_pn.lock().unwrap();
        if a.ends_with("@lid") && !b.ends_with("@lid") {
            map.insert(a, b);
        } else if b.ends_with("@lid") && !a.ends_with("@lid") {
            map.insert(b, a);
        }
    }

    /// Record a direct LID→PN string mapping.
    pub fn record_lid_to_pn(&self, lid_jid_str: &str, pn_jid_str: &str) {
        let mut map = self.lid_to_pn.lock().unwrap();
        map.insert(lid_jid_str.to_string(), pn_jid_str.to_string());
    }

    /// Resolve a JID string: if it's a LID with a known PN, return the PN string.
    /// Otherwise return the original string.
    pub fn normalize_jid_str(&self, jid_str: &str) -> String {
        if jid_str.ends_with("@lid") {
            let map = self.lid_to_pn.lock().unwrap();
            if let Some(pn) = map.get(jid_str) {
                return pn.clone();
            }
        }
        jid_str.to_string()
    }
}

/// Convert a WhatsApp JID to our chat_id string format, normalizing LID→PN.
pub fn jid_to_chat_id(jid: &Jid, cache: &JidCache) -> String {
    let jid_str = jid.to_string();
    let normalized = cache.normalize_jid_str(&jid_str);
    format!("wa-{}", normalized)
}

/// Convert our chat_id string back to a WhatsApp JID.
pub fn chat_id_to_jid(chat_id: &str) -> Option<Jid> {
    let raw = chat_id.strip_prefix("wa-")?;
    raw.parse::<Jid>().ok()
}

/// Convert a WhatsApp message + info into our UnifiedMessage.
pub fn wa_message_to_unified(
    msg: &wa::Message,
    push_name: &str,
    msg_id: &str,
    timestamp: chrono::DateTime<chrono::Utc>,
    chat_jid: &Jid,
    sender_jid: &Jid,
    is_from_me: bool,
    _is_group: bool,
    jid_cache: &JidCache,
) -> Option<UnifiedMessage> {
    let content = extract_message_content(msg)?;

    let chat_id = jid_to_chat_id(chat_jid, jid_cache);
    let sender = if is_from_me {
        "You".to_string()
    } else if !push_name.is_empty() {
        push_name.to_string()
    } else {
        sender_jid.to_string()
    };

    Some(UnifiedMessage {
        id: msg_id.to_string(),
        chat_id,
        platform: Platform::WhatsApp,
        sender,
        content,
        timestamp,
        status: MessageStatus::Delivered,
        is_outgoing: is_from_me,
    })
}

/// Convert a WebMessageInfo (from history sync) into our UnifiedMessage.
pub fn web_msg_to_unified(
    web_msg: &wa::WebMessageInfo,
    jid_cache: &JidCache,
) -> Option<UnifiedMessage> {
    let key = &web_msg.key;
    let msg_id = key.id.as_ref()?;
    let remote_jid_str = key.remote_jid.as_ref()?;
    let is_from_me = key.from_me.unwrap_or(false);

    let timestamp_secs = web_msg.message_timestamp.unwrap_or(0) as i64;
    let timestamp = chrono::DateTime::from_timestamp(timestamp_secs, 0)?;

    let wa_msg = web_msg.message.as_ref()?;
    let content = extract_message_content(wa_msg)?;

    let normalized_jid_str = jid_cache.normalize_jid_str(remote_jid_str);
    let chat_id = format!("wa-{}", normalized_jid_str);

    let sender = if is_from_me {
        "You".to_string()
    } else if let Some(ref pn) = web_msg.push_name {
        if !pn.is_empty() {
            pn.clone()
        } else {
            strip_jid_server(remote_jid_str)
        }
    } else if let Some(ref participant) = key.participant {
        strip_jid_server(participant)
    } else {
        strip_jid_server(remote_jid_str)
    };

    let status = match web_msg.status {
        Some(0) => MessageStatus::Failed,
        Some(1) => MessageStatus::Sending,
        Some(2) => MessageStatus::Sent,
        Some(3) => MessageStatus::Delivered,
        Some(4) | Some(5) => MessageStatus::Read,
        _ => {
            if is_from_me {
                MessageStatus::Sent
            } else {
                MessageStatus::Delivered
            }
        }
    };

    Some(UnifiedMessage {
        id: msg_id.clone(),
        chat_id,
        platform: Platform::WhatsApp,
        sender,
        content,
        timestamp,
        status,
        is_outgoing: is_from_me,
    })
}

/// Extract a human-readable display name from a JID.
/// Strips the @server part, giving just the phone number or group ID.
pub fn jid_to_display_name(jid: &Jid) -> String {
    let full = jid.to_string();
    strip_jid_server(&full)
}

/// Build a WhatsApp text message protobuf from a string.
pub fn text_to_wa_message(text: &str) -> wa::Message {
    wa::Message {
        conversation: Some(text.to_string()),
        ..Default::default()
    }
}

/// Extract message content from a wa::Message, returning None for unsupported types.
fn extract_message_content(msg: &wa::Message) -> Option<MessageContent> {
    let text = msg.text_content().or_else(|| msg.get_caption());
    match text {
        Some(t) => Some(MessageContent::Text(t.to_string())),
        None => {
            let base = msg.get_base_message();
            if base.image_message.is_some() {
                Some(MessageContent::Text("[Image]".to_string()))
            } else if base.document_message.is_some() {
                Some(MessageContent::Text("[Document]".to_string()))
            } else if base.audio_message.is_some() {
                Some(MessageContent::Text("[Audio]".to_string()))
            } else if base.video_message.is_some() {
                Some(MessageContent::Text("[Video]".to_string()))
            } else if base.sticker_message.is_some() {
                Some(MessageContent::Text("[Sticker]".to_string()))
            } else if base.contact_message.is_some() {
                Some(MessageContent::Text("[Contact]".to_string()))
            } else if base.location_message.is_some() {
                Some(MessageContent::Text("[Location]".to_string()))
            } else {
                None
            }
        }
    }
}

/// Strip the @server suffix from a JID string.
fn strip_jid_server(jid_str: &str) -> String {
    jid_str.split('@').next().unwrap_or(jid_str).to_string()
}
