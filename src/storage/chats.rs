use crate::core::types::{Platform, UnifiedChat};
use crate::core::Result;
use crate::storage::db::Database;

impl Database {
    pub fn upsert_chat(&self, chat: &UnifiedChat) -> Result<()> {
        let platform_str = format!("{:?}", chat.platform);
        self.conn.execute(
            "INSERT OR REPLACE INTO chats (id, platform, name, last_message, unread_count, is_group, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
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
            "SELECT id, platform, name, last_message, unread_count, is_group
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

                Ok((id, platform_str, name, last_message, unread_count, is_group))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let result = chats
            .into_iter()
            .map(|(id, platform_str, name, last_message, unread_count, is_group)| {
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

    pub fn update_last_message(&self, chat_id: &str, last_message: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE chats SET last_message = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![last_message, chat_id],
        )?;
        Ok(())
    }
}
