use crate::core::Result;
use crate::storage::db::Database;

impl Database {
    pub fn save_session(&self, provider: &str, data: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO sessions (provider, data, updated_at)
             VALUES (?1, ?2, datetime('now'))",
            rusqlite::params![provider, data],
        )?;
        Ok(())
    }

    pub fn get_session(&self, provider: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM sessions WHERE provider = ?1")?;
        let mut rows = stmt.query_map(rusqlite::params![provider], |row| row.get(0))?;
        match rows.next() {
            Some(Ok(data)) => Ok(Some(data)),
            _ => Ok(None),
        }
    }

    pub fn delete_session(&self, provider: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM sessions WHERE provider = ?1",
            rusqlite::params![provider],
        )?;
        Ok(())
    }
}
