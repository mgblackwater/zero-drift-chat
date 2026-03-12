use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use whatsapp_rust::proto_helpers::MessageExt;
use whatsapp_rust::waproto::whatsapp as wa;
use whatsapp_rust::Jid;

use crate::core::provider::ProviderEvent;
use crate::core::types::*;

/// Cache mapping LID JID strings to their PN (phone number) JID equivalents.
/// WhatsApp uses two JID formats for the same person:
/// - PN: `559985213786@s.whatsapp.net`
/// - LID: `39492358562039@lid`
///
/// When a new mapping is discovered the cache emits a
/// `ProviderEvent::LidPnMappingDiscovered` so the app layer can persist it and
/// remove any stale `@lid` chat entry.
#[derive(Clone)]
pub struct JidCache {
    lid_to_pn: Arc<Mutex<HashMap<String, String>>>,
    /// Optional channel used to notify the app of newly discovered mappings.
    tx: Option<mpsc::UnboundedSender<ProviderEvent>>,
}

impl Default for JidCache {
    fn default() -> Self {
        Self {
            lid_to_pn: Arc::new(Mutex::new(HashMap::new())),
            tx: None,
        }
    }
}

impl JidCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-populate the cache with previously persisted mappings (loaded from DB
    /// on startup) so that LID JIDs are normalised correctly from the first event.
    pub fn new_with_mappings(
        map: HashMap<String, String>,
        tx: mpsc::UnboundedSender<ProviderEvent>,
    ) -> Self {
        Self {
            lid_to_pn: Arc::new(Mutex::new(map)),
            tx: Some(tx),
        }
    }

    /// Record a mapping between two JIDs (auto-detects LID vs PN).
    /// Emits `LidPnMappingDiscovered` when a genuinely new mapping is added.
    pub fn record_mapping(&self, jid_a: &Jid, jid_b: &Jid) {
        let a = jid_a.to_string();
        let b = jid_b.to_string();
        if a.ends_with("@lid") && !b.ends_with("@lid") {
            self.insert_if_new(a, b);
        } else if b.ends_with("@lid") && !a.ends_with("@lid") {
            self.insert_if_new(b, a);
        }
    }

    /// Record a direct LID→PN string mapping.
    /// Emits `LidPnMappingDiscovered` when a genuinely new mapping is added.
    pub fn record_lid_to_pn(&self, lid_jid_str: &str, pn_jid_str: &str) {
        self.insert_if_new(lid_jid_str.to_string(), pn_jid_str.to_string());
    }

    /// Insert lid→pn only if not already present; emit event for new entries.
    fn insert_if_new(&self, lid: String, pn: String) {
        let is_new = {
            let mut map = self.lid_to_pn.lock().unwrap();
            if map.contains_key(&lid) {
                false
            } else {
                map.insert(lid.clone(), pn.clone());
                true
            }
        };
        if is_new {
            if let Some(ref tx) = self.tx {
                let _ = tx.send(ProviderEvent::LidPnMappingDiscovered {
                    lid: lid.clone(),
                    pn: pn.clone(),
                });
            }
        }
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
#[allow(clippy::too_many_arguments)]
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
    // Reactions — show as a simple line
    if let Some(ref reaction) = msg.reaction_message {
        let emoji = reaction.text.as_deref().unwrap_or("");
        if emoji.is_empty() {
            return Some(MessageContent::Text("removed a reaction".to_string()));
        }
        return Some(MessageContent::Text(format!("reacted {}", emoji)));
    }

    // Plain text (conversation or extended text message)
    if let Some(text) = msg.text_content() {
        return Some(MessageContent::Text(text.to_string()));
    }

    // Media types with enriched metadata
    let base = msg.get_base_message();

    if let Some(ref img) = base.image_message {
        return Some(MessageContent::Text(
            match img.caption.as_deref().filter(|s| !s.is_empty()) {
                Some(c) => format!("[Image] {}", c),
                None => "[Image]".to_string(),
            },
        ));
    }
    if let Some(ref vid) = base.video_message {
        let mut label = "[Video".to_string();
        if let Some(s) = vid.seconds {
            label.push_str(&format!(" {}:{:02}", s / 60, s % 60));
        }
        label.push(']');
        if let Some(c) = vid.caption.as_deref().filter(|s| !s.is_empty()) {
            label.push(' ');
            label.push_str(c);
        }
        return Some(MessageContent::Text(label));
    }
    if let Some(ref doc) = base.document_message {
        let name = doc
            .file_name
            .as_deref()
            .or(doc.title.as_deref())
            .filter(|s| !s.is_empty());
        return Some(MessageContent::Text(match name {
            Some(n) => format!("[Document] {}", n),
            None => "[Document]".to_string(),
        }));
    }
    if let Some(ref audio) = base.audio_message {
        let tag = if audio.ptt.unwrap_or(false) {
            "Voice"
        } else {
            "Audio"
        };
        return Some(MessageContent::Text(match audio.seconds {
            Some(s) => format!("[{}] {}:{:02}", tag, s / 60, s % 60),
            None => format!("[{}]", tag),
        }));
    }
    if base.sticker_message.is_some() {
        return Some(MessageContent::Text("[Sticker]".to_string()));
    }
    if base.contact_message.is_some() {
        return Some(MessageContent::Text("[Contact]".to_string()));
    }
    if base.location_message.is_some() {
        return Some(MessageContent::Text("[Location]".to_string()));
    }

    None
}

/// Strip the @server suffix from a JID string.
fn strip_jid_server(jid_str: &str) -> String {
    jid_str.split('@').next().unwrap_or(jid_str).to_string()
}
