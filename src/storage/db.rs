use rusqlite::Connection;

use crate::core::Result;

pub struct Database {
    pub conn: Connection,
}

impl Database {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    #[allow(dead_code)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS chats (
                id TEXT PRIMARY KEY,
                platform TEXT NOT NULL,
                name TEXT NOT NULL,
                last_message TEXT,
                unread_count INTEGER NOT NULL DEFAULT 0,
                is_group INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS contacts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                platform TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                chat_id TEXT NOT NULL,
                platform TEXT NOT NULL,
                sender TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                status TEXT NOT NULL,
                is_outgoing INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (chat_id) REFERENCES chats(id)
            );

            CREATE INDEX IF NOT EXISTS idx_messages_chat_id ON messages(chat_id);
            CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);

            CREATE TABLE IF NOT EXISTS sessions (
                provider TEXT PRIMARY KEY,
                data TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            ",
        )?;

        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS preferences (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;

        // Migration: add display_name column if not exists
        let _ = self
            .conn
            .execute("ALTER TABLE chats ADD COLUMN display_name TEXT", []);

        // Migration: add pinned column if not exists
        let _ = self.conn.execute(
            "ALTER TABLE chats ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
            [],
        );

        // Migration: add is_newsletter column if not exists
        let _ = self.conn.execute(
            "ALTER TABLE chats ADD COLUMN is_newsletter INTEGER NOT NULL DEFAULT 0",
            [],
        );

        // Backfill is_newsletter for chats already in DB whose id contains @newsletter
        let _ = self.conn.execute(
            "UPDATE chats SET is_newsletter = 1 WHERE id LIKE '%@newsletter%' AND is_newsletter = 0",
            [],
        );

        // Migration: add muted column if not exists
        let _ = self.conn.execute(
            "ALTER TABLE chats ADD COLUMN muted INTEGER NOT NULL DEFAULT 0",
            [],
        );

        // Migration: create scheduled_messages table
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS scheduled_messages (
                id TEXT PRIMARY KEY,
                chat_id TEXT NOT NULL,
                platform TEXT NOT NULL,
                content TEXT NOT NULL,
                send_at TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL,
                FOREIGN KEY (chat_id) REFERENCES chats(id)
            );

            CREATE INDEX IF NOT EXISTS idx_scheduled_pending
                ON scheduled_messages(status, send_at)
                WHERE status = 'pending';
            ",
        )?;

        // Migration: add kind column
        let _ = self.conn.execute(
            "ALTER TABLE chats ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat'",
            [],
        );

        // Migration: backfill kind from is_newsletter / is_group
        let _ = self.conn.execute(
            "UPDATE chats SET kind = 'newsletter' WHERE is_newsletter = 1 AND kind = 'chat'",
            [],
        );
        let _ = self.conn.execute(
            "UPDATE chats SET kind = 'group' WHERE is_group = 1 AND kind = 'chat'",
            [],
        );

        // Migration: drop is_group / is_newsletter via recreate-table (SQLite doesn't
        // support DROP COLUMN reliably). Guard: only run if is_group column still exists.
        let has_is_group: bool = {
            let mut stmt = self
                .conn
                .prepare("PRAGMA table_info(chats)")
                .expect("PRAGMA table_info failed");
            let cols: Vec<_> = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .expect("query_map failed")
                .collect();
            cols.into_iter()
                .any(|col| col.map(|s| s == "is_group").unwrap_or(false))
        };

        if has_is_group {
            self.conn
                .execute_batch(
                    "
                BEGIN;
                CREATE TABLE chats_new (
                    id           TEXT    PRIMARY KEY,
                    platform     TEXT    NOT NULL,
                    name         TEXT    NOT NULL,
                    last_message TEXT,
                    unread_count INTEGER NOT NULL DEFAULT 0,
                    kind         TEXT    NOT NULL DEFAULT 'chat',
                    updated_at   TEXT    NOT NULL DEFAULT (datetime('now')),
                    display_name TEXT,
                    pinned       INTEGER NOT NULL DEFAULT 0,
                    muted        INTEGER NOT NULL DEFAULT 0
                );
                INSERT INTO chats_new
                    SELECT id, platform, name, last_message, unread_count,
                           kind, updated_at, display_name, pinned, muted
                    FROM chats;
                DROP TABLE chats;
                ALTER TABLE chats_new RENAME TO chats;
                COMMIT;
            ",
                )
                .expect("chats recreate-table migration failed");
        }

        // Migration: create lid_pn_map table for persisting WhatsApp LID→PN JID mappings.
        // Without this, every restart loses the mapping and the same person appears twice
        // (once as wa-<phone>@s.whatsapp.net, once as wa-<lid>@lid).
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS lid_pn_map (
                lid TEXT PRIMARY KEY,
                pn  TEXT NOT NULL
            );",
        )?;

        // Migration: delete stale @lid chat rows whose LID is now mapped to a PN.
        // This cleans up duplicates created before the lid_pn_map table existed.
        let _ = self.conn.execute(
            "DELETE FROM chats
             WHERE id LIKE 'wa-%@lid'
               AND EXISTS (
                   SELECT 1 FROM lid_pn_map
                    WHERE 'wa-' || lid = chats.id
               )",
            [],
        );

        Ok(())
    }
}
