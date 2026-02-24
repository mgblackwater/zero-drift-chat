use crate::core::types::{MessageContent, MessageStatus, Platform, UnifiedMessage};
use crate::core::Result;
use crate::storage::db::Database;
use chrono::DateTime;

impl Database {
    pub fn insert_message(&self, msg: &UnifiedMessage) -> Result<()> {
        let content_json = serde_json::to_string(&msg.content)?;
        let status_str = format!("{:?}", msg.status);
        let platform_str = format!("{:?}", msg.platform);
        let timestamp_str = msg.timestamp.to_rfc3339();

        self.conn.execute(
            "INSERT OR REPLACE INTO messages (id, chat_id, platform, sender, content, timestamp, status, is_outgoing)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                msg.id,
                msg.chat_id,
                platform_str,
                msg.sender,
                content_json,
                timestamp_str,
                status_str,
                msg.is_outgoing as i32,
            ],
        )?;
        Ok(())
    }

    pub fn get_messages_for_chat(&self, chat_id: &str) -> Result<Vec<UnifiedMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, chat_id, platform, sender, content, timestamp, status, is_outgoing
             FROM messages WHERE chat_id = ?1 ORDER BY timestamp ASC",
        )?;

        let messages = stmt
            .query_map(rusqlite::params![chat_id], |row| {
                let id: String = row.get(0)?;
                let chat_id: String = row.get(1)?;
                let platform_str: String = row.get(2)?;
                let sender: String = row.get(3)?;
                let content_json: String = row.get(4)?;
                let timestamp_str: String = row.get(5)?;
                let status_str: String = row.get(6)?;
                let is_outgoing: i32 = row.get(7)?;

                Ok((
                    id,
                    chat_id,
                    platform_str,
                    sender,
                    content_json,
                    timestamp_str,
                    status_str,
                    is_outgoing,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut result = Vec::new();
        for (id, chat_id, platform_str, sender, content_json, timestamp_str, status_str, is_outgoing) in messages {
            let platform = parse_platform(&platform_str);
            let content: MessageContent = serde_json::from_str(&content_json)
                .unwrap_or(MessageContent::Text(content_json));
            let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            let status = parse_status(&status_str);

            result.push(UnifiedMessage {
                id,
                chat_id,
                platform,
                sender,
                content,
                timestamp,
                status,
                is_outgoing: is_outgoing != 0,
            });
        }

        Ok(result)
    }

    pub fn update_message_status(&self, message_id: &str, status: MessageStatus) -> Result<()> {
        let status_str = format!("{:?}", status);
        self.conn.execute(
            "UPDATE messages SET status = ?1 WHERE id = ?2",
            rusqlite::params![status_str, message_id],
        )?;
        Ok(())
    }
}

fn parse_platform(s: &str) -> Platform {
    match s {
        "WhatsApp" => Platform::WhatsApp,
        "Telegram" => Platform::Telegram,
        "Slack" => Platform::Slack,
        _ => Platform::Mock,
    }
}

fn parse_status(s: &str) -> MessageStatus {
    match s {
        "Sending" => MessageStatus::Sending,
        "Sent" => MessageStatus::Sent,
        "Delivered" => MessageStatus::Delivered,
        "Read" => MessageStatus::Read,
        "Failed" => MessageStatus::Failed,
        _ => MessageStatus::Sent,
    }
}
