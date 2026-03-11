use crate::core::types::{MessageContent, Platform};
use crate::core::Result;
use crate::storage::db::Database;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct ScheduledMessage {
    pub id: String,
    pub chat_id: String,
    pub platform: Platform,
    pub content: MessageContent,
    pub send_at: DateTime<Utc>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

struct ScheduledMessageRow {
    id: String,
    chat_id: String,
    platform_str: String,
    content_json: String,
    send_at_str: String,
    status: String,
    created_at_str: String,
}

fn parse_platform(s: &str) -> Platform {
    match s {
        "WhatsApp" => Platform::WhatsApp,
        "Telegram" => Platform::Telegram,
        "Slack" => Platform::Slack,
        _ => Platform::Mock,
    }
}

fn parse_scheduled_row(row: ScheduledMessageRow) -> ScheduledMessage {
    let platform = parse_platform(&row.platform_str);
    let content: MessageContent =
        serde_json::from_str(&row.content_json).unwrap_or(MessageContent::Text(row.content_json));
    let send_at = DateTime::parse_from_rfc3339(&row.send_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let created_at = DateTime::parse_from_rfc3339(&row.created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    ScheduledMessage {
        id: row.id,
        chat_id: row.chat_id,
        platform,
        content,
        send_at,
        status: row.status,
        created_at,
    }
}

impl Database {
    pub fn insert_scheduled_message(&self, msg: &ScheduledMessage) -> Result<()> {
        let content_json = serde_json::to_string(&msg.content)?;
        let platform_str = format!("{:?}", msg.platform);
        let send_at_str = msg.send_at.to_rfc3339();
        let created_at_str = msg.created_at.to_rfc3339();

        self.conn.execute(
            "INSERT INTO scheduled_messages (id, chat_id, platform, content, send_at, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                msg.id,
                msg.chat_id,
                platform_str,
                content_json,
                send_at_str,
                msg.status,
                created_at_str,
            ],
        )?;
        Ok(())
    }

    pub fn get_due_scheduled_messages(&self) -> Result<Vec<ScheduledMessage>> {
        let now = Utc::now().to_rfc3339();
        let mut stmt = self.conn.prepare(
            "SELECT id, chat_id, platform, content, send_at, status, created_at
             FROM scheduled_messages
             WHERE status = 'pending' AND send_at <= ?1
             ORDER BY send_at ASC",
        )?;

        let rows = stmt
            .query_map(rusqlite::params![now], |row| {
                Ok(ScheduledMessageRow {
                    id: row.get(0)?,
                    chat_id: row.get(1)?,
                    platform_str: row.get(2)?,
                    content_json: row.get(3)?,
                    send_at_str: row.get(4)?,
                    status: row.get(5)?,
                    created_at_str: row.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows.into_iter().map(parse_scheduled_row).collect())
    }

    pub fn get_pending_scheduled_messages(&self) -> Result<Vec<ScheduledMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, chat_id, platform, content, send_at, status, created_at
             FROM scheduled_messages
             WHERE status = 'pending'
             ORDER BY send_at ASC",
        )?;

        let rows = stmt
            .query_map([], |row| {
                Ok(ScheduledMessageRow {
                    id: row.get(0)?,
                    chat_id: row.get(1)?,
                    platform_str: row.get(2)?,
                    content_json: row.get(3)?,
                    send_at_str: row.get(4)?,
                    status: row.get(5)?,
                    created_at_str: row.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows.into_iter().map(parse_scheduled_row).collect())
    }

    pub fn update_scheduled_status(&self, id: &str, status: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE scheduled_messages SET status = ?1 WHERE id = ?2",
            rusqlite::params![status, id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::UnifiedChat;
    use chrono::Duration;

    fn setup_db() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.upsert_chat(&UnifiedChat {
            id: "chat-1".to_string(),
            platform: Platform::Mock,
            name: "Test Chat".to_string(),
            display_name: None,
            last_message: None,
            unread_count: 0,
            is_group: false,
            is_pinned: false,
            is_newsletter: false,
            is_muted: false,
        })
        .unwrap();
        db
    }

    fn make_test_message(id: &str, send_at: DateTime<Utc>) -> ScheduledMessage {
        ScheduledMessage {
            id: id.to_string(),
            chat_id: "chat-1".to_string(),
            platform: Platform::Mock,
            content: MessageContent::Text("Hello scheduled".to_string()),
            send_at,
            status: "pending".to_string(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn insert_and_query_due() {
        let db = setup_db();
        let past = Utc::now() - Duration::minutes(5);
        let msg = make_test_message("sched-1", past);

        db.insert_scheduled_message(&msg).unwrap();

        let due = db.get_due_scheduled_messages().unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "sched-1");
        assert_eq!(due[0].content.as_text(), "Hello scheduled");
    }

    #[test]
    fn future_message_not_due() {
        let db = setup_db();
        let future = Utc::now() + Duration::hours(1);
        let msg = make_test_message("sched-2", future);

        db.insert_scheduled_message(&msg).unwrap();

        let due = db.get_due_scheduled_messages().unwrap();
        assert!(due.is_empty());
    }

    #[test]
    fn cancel_removes_from_due() {
        let db = setup_db();
        let past = Utc::now() - Duration::minutes(5);
        let msg = make_test_message("sched-3", past);

        db.insert_scheduled_message(&msg).unwrap();
        db.update_scheduled_status("sched-3", "cancelled").unwrap();

        let due = db.get_due_scheduled_messages().unwrap();
        assert!(due.is_empty());
    }

    #[test]
    fn list_pending_includes_future() {
        let db = setup_db();
        let future = Utc::now() + Duration::hours(1);
        let msg = make_test_message("sched-4", future);

        db.insert_scheduled_message(&msg).unwrap();

        let pending = db.get_pending_scheduled_messages().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "sched-4");
    }
}
