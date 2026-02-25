use whatsapp_rust::proto_helpers::MessageExt;
use whatsapp_rust::waproto::whatsapp as wa;
use whatsapp_rust::Jid;

use crate::core::types::*;

/// Convert a WhatsApp JID to our chat_id string format.
pub fn jid_to_chat_id(jid: &Jid) -> String {
    format!("wa-{}", jid)
}

/// Convert our chat_id string back to a WhatsApp JID.
pub fn chat_id_to_jid(chat_id: &str) -> Option<Jid> {
    let raw = chat_id.strip_prefix("wa-")?;
    raw.parse::<Jid>().ok()
}

/// Convert a WhatsApp message + info into our UnifiedMessage.
pub fn wa_message_to_unified(msg: &wa::Message, push_name: &str, msg_id: &str, timestamp: chrono::DateTime<chrono::Utc>, chat_jid: &Jid, sender_jid: &Jid, is_from_me: bool, _is_group: bool) -> Option<UnifiedMessage> {
    let text = msg.text_content().or_else(|| msg.get_caption());
    let content = match text {
        Some(t) => MessageContent::Text(t.to_string()),
        None => {
            let base = msg.get_base_message();
            if base.image_message.is_some() {
                MessageContent::Text("[Image]".to_string())
            } else if base.document_message.is_some() {
                MessageContent::Text("[Document]".to_string())
            } else if base.audio_message.is_some() {
                MessageContent::Text("[Audio]".to_string())
            } else if base.video_message.is_some() {
                MessageContent::Text("[Video]".to_string())
            } else if base.sticker_message.is_some() {
                MessageContent::Text("[Sticker]".to_string())
            } else if base.contact_message.is_some() {
                MessageContent::Text("[Contact]".to_string())
            } else if base.location_message.is_some() {
                MessageContent::Text("[Location]".to_string())
            } else {
                return None;
            }
        }
    };

    let chat_id = jid_to_chat_id(chat_jid);
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

/// Extract a human-readable display name from a JID.
/// Strips the @server part, giving just the phone number or group ID.
pub fn jid_to_display_name(jid: &Jid) -> String {
    let full = jid.to_string();
    // Strip @s.whatsapp.net, @g.us, @lid, etc.
    full.split('@').next().unwrap_or(&full).to_string()
}

/// Build a WhatsApp text message protobuf from a string.
pub fn text_to_wa_message(text: &str) -> wa::Message {
    wa::Message {
        conversation: Some(text.to_string()),
        ..Default::default()
    }
}
