use std::collections::HashMap;

use crate::core::Result;
use crate::storage::db::Database;

impl Database {
    /// Persist a single LID→PN mapping so it survives app restarts.
    pub fn save_lid_mapping(&self, lid: &str, pn: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO lid_pn_map (lid, pn) VALUES (?1, ?2)
             ON CONFLICT(lid) DO UPDATE SET pn = excluded.pn",
            rusqlite::params![lid, pn],
        )?;
        Ok(())
    }

    /// Load all persisted LID→PN mappings.  Returns a map of `lid → pn` strings
    /// (without the `wa-` prefix — raw JID strings as stored).
    pub fn load_lid_mappings(&self) -> Result<HashMap<String, String>> {
        let mut stmt = self.conn.prepare("SELECT lid, pn FROM lid_pn_map")?;
        let map = stmt
            .query_map([], |row| {
                let lid: String = row.get(0)?;
                let pn: String = row.get(1)?;
                Ok((lid, pn))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(map)
    }

    /// Delete a stale @lid chat row now that we know the PN equivalent.
    /// Called after recording a new LID→PN mapping to remove any duplicate entry.
    pub fn delete_lid_chat(&self, lid_chat_id: &str) -> Result<()> {
        // Only delete if the PN version exists — avoid orphaning messages.
        self.conn.execute(
            "DELETE FROM chats WHERE id = ?1",
            rusqlite::params![lid_chat_id],
        )?;
        Ok(())
    }
}
