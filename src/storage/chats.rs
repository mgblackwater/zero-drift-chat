use crate::core::types::{ChatKind, Platform, UnifiedChat};
use crate::core::Result;
use crate::storage::db::Database;

impl Database {
    pub fn upsert_chat(&self, chat: &UnifiedChat) -> Result<()> {
        let platform_str = format!("{:?}", chat.platform);
        self.conn.execute(
            "INSERT INTO chats (id, platform, name, last_message, unread_count, kind, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
               platform     = excluded.platform,
               name         = excluded.name,
               last_message = COALESCE(excluded.last_message, chats.last_message),
               unread_count = excluded.unread_count,
               kind         = excluded.kind,
               updated_at   = datetime('now')",
            rusqlite::params![
                chat.id,
                platform_str,
                chat.name,
                chat.last_message,
                chat.unread_count,
                chat.kind.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn get_all_chats(&self) -> Result<Vec<UnifiedChat>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, platform, name, last_message, unread_count, kind,
                    display_name, pinned, muted
             FROM chats ORDER BY pinned DESC, updated_at DESC",
        )?;

        let chats = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let platform_str: String = row.get(1)?;
                let name: String = row.get(2)?;
                let last_message: Option<String> = row.get(3)?;
                let unread_count: u32 = row.get(4)?;
                let kind_str: String = row.get(5)?;
                let display_name: Option<String> = row.get(6)?;
                let pinned: i32 = row.get(7)?;
                let muted: i32 = row.get(8)?;
                Ok((
                    id,
                    platform_str,
                    name,
                    last_message,
                    unread_count,
                    kind_str,
                    display_name,
                    pinned,
                    muted,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let result = chats
            .into_iter()
            .map(
                |(
                    id,
                    platform_str,
                    name,
                    last_message,
                    unread_count,
                    kind_str,
                    display_name,
                    pinned,
                    muted,
                )| {
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
                        kind: ChatKind::from_str(&kind_str),
                        is_pinned: pinned != 0,
                        is_muted: muted != 0,
                    }
                },
            )
            .collect();

        Ok(result)
    }

    pub fn set_chat_muted(&self, chat_id: &str, muted: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE chats SET muted = ?1 WHERE id = ?2",
            rusqlite::params![muted as i32, chat_id],
        )?;
        Ok(())
    }

    pub fn set_chat_pinned(&self, chat_id: &str, pinned: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE chats SET pinned = ?1 WHERE id = ?2",
            rusqlite::params![pinned as i32, chat_id],
        )?;
        Ok(())
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
