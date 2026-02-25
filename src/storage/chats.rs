use crate::core::types::{Platform, UnifiedChat};
use crate::core::Result;
use crate::storage::db::Database;

impl Database {
    pub fn upsert_chat(&self, chat: &UnifiedChat) -> Result<()> {
        let platform_str = format!("{:?}", chat.platform);
        // Use INSERT ON CONFLICT to preserve user-set display_name
        self.conn.execute(
            "INSERT INTO chats (id, platform, name, last_message, unread_count, is_group, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
               platform = excluded.platform,
               name = excluded.name,
               last_message = COALESCE(excluded.last_message, chats.last_message),
               unread_count = excluded.unread_count,
               is_group = excluded.is_group,
               updated_at = datetime('now')",
            rusqlite::params![
                chat.id,
                platform_str,
                chat.name,
                chat.last_message,
                chat.unread_count,
                chat.is_group as i32,
            ],
        )?;
        Ok(())
    }

    pub fn get_all_chats(&self) -> Result<Vec<UnifiedChat>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, platform, name, last_message, unread_count, is_group, display_name
             FROM chats ORDER BY updated_at DESC",
        )?;

        let chats = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let platform_str: String = row.get(1)?;
                let name: String = row.get(2)?;
                let last_message: Option<String> = row.get(3)?;
                let unread_count: u32 = row.get(4)?;
                let is_group: i32 = row.get(5)?;
                let display_name: Option<String> = row.get(6)?;

                Ok((id, platform_str, name, last_message, unread_count, is_group, display_name))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let result = chats
            .into_iter()
            .map(|(id, platform_str, name, last_message, unread_count, is_group, display_name)| {
                let platform = match platform_str.as_str() {
                    "WhatsApp" => Platform::WhatsApp,
                    "Telegram" => Platform::Telegram,
                    "Slack" => Platform::Slack,
                    _ => Platform::Mock,
                };
                UnifiedChat {
                    id,
                    platform,
                    name,
                    display_name,
                    last_message,
                    unread_count,
                    is_group: is_group != 0,
                }
            })
            .collect();

        Ok(result)
    }

    pub fn update_unread_count(&self, chat_id: &str, count: u32) -> Result<()> {
        self.conn.execute(
            "UPDATE chats SET unread_count = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![count, chat_id],
        )?;
        Ok(())
    }

    pub fn set_display_name(&self, chat_id: &str, display_name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE chats SET display_name = ?1 WHERE id = ?2",
            rusqlite::params![display_name, chat_id],
        )?;
        Ok(())
    }

    pub fn update_last_message(&self, chat_id: &str, last_message: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE chats SET last_message = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![last_message, chat_id],
        )?;
        Ok(())
    }
}
