use std::collections::HashMap;

use rusqlite::Connection;

use crate::core::Result;

pub struct AddressBook {
    conn: Connection,
}

impl AddressBook {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let ab = Self { conn };
        ab.migrate()?;
        Ok(ab)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS display_names (
                chat_id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    pub fn get_all_display_names(&self) -> Result<HashMap<String, String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT chat_id, display_name FROM display_names")?;
        let rows = stmt
            .query_map([], |row| {
                let chat_id: String = row.get(0)?;
                let display_name: String = row.get(1)?;
                Ok((chat_id, display_name))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().collect())
    }

    pub fn set_display_name(&self, chat_id: &str, display_name: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO display_names (chat_id, display_name)
             VALUES (?1, ?2)
             ON CONFLICT(chat_id) DO UPDATE SET display_name = excluded.display_name",
            rusqlite::params![chat_id, display_name],
        )?;
        Ok(())
    }
}
