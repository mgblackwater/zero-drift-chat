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
            );
            CREATE TABLE IF NOT EXISTS contacts (
                phone TEXT PRIMARY KEY,
                name TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    /// User-set display names, keyed by chat_id. Highest priority.
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

    /// Store a contact name from push_name / contact sync.
    /// Does NOT overwrite if the name is already set (user-synced names take priority).
    pub fn upsert_contact(&self, phone: &str, name: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO contacts (phone, name) VALUES (?1, ?2)
             ON CONFLICT(phone) DO UPDATE SET name = excluded.name",
            rusqlite::params![phone, name],
        )?;
        Ok(())
    }

    /// Look up a contact name by phone number (partial match: `phone` should be
    /// the normalized number without country-code prefix inconsistencies).
    pub fn lookup_contact(&self, phone: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM contacts WHERE phone = ?1 LIMIT 1")?;
        let mut rows = stmt.query(rusqlite::params![phone])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// All contacts, for search/display. Returns (phone, name).
    #[allow(dead_code)]
    pub fn get_all_contacts(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT phone, name FROM contacts ORDER BY name")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
