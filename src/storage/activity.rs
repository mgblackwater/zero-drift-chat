// src/storage/activity.rs
use std::collections::HashMap;

use crate::storage::db::Database;

/// Encode a 24-hour hourly bucket array into a 10-character Braille sparkline.
/// `array[23]` = current hour, `array[0]` = 23 hours ago.
/// Renders the most recent 10 hours (indices 14..=23).
pub fn encode_braille(array: &[u32; 24]) -> String {
    const BRAILLE: [char; 9] = ['⠀', '⣀', '⣄', '⣆', '⣇', '⣧', '⣷', '⣾', '⣿'];
    let slice = &array[14..]; // 10 elements: indices 14..=23
    let max_val = *slice.iter().max().unwrap_or(&0);
    if max_val == 0 {
        return BRAILLE[0].to_string().repeat(10);
    }
    slice
        .iter()
        .map(|&v| BRAILLE[((v * 8) / max_val).min(8) as usize])
        .collect()
}

/// Query 24-hour message activity bucketed by hour for a list of chat IDs.
/// Returns chat_id → [u32; 24] where index 23 = current hour, index 0 = 23 hours ago.
/// Returns an empty map if `chat_ids` is empty.
pub fn query_activity_24h(db: &Database, chat_ids: &[&str]) -> HashMap<String, [u32; 24]> {
    if chat_ids.is_empty() {
        return HashMap::new();
    }

    let placeholders: String = chat_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");

    // strftime('%s', ...) has broader SQLite compatibility than unixepoch()
    // (works from SQLite 3.8+, whereas unixepoch() requires 3.38+).
    // Timestamps are stored as RFC 3339 strings.
    let sql = format!(
        "SELECT chat_id,
                CAST((CAST(strftime('%s', 'now') AS INTEGER)
                      - CAST(strftime('%s', timestamp) AS INTEGER)) / 3600 AS INTEGER) AS hours_ago,
                COUNT(*) AS cnt
         FROM messages
         WHERE CAST(strftime('%s', timestamp) AS INTEGER)
               > CAST(strftime('%s', 'now') AS INTEGER) - 86400
           AND chat_id IN ({placeholders})
         GROUP BY chat_id, hours_ago"
    );

    let mut result: HashMap<String, [u32; 24]> = HashMap::new();

    let query_result = db.conn.prepare(&sql).and_then(|mut stmt| {
        let params: Vec<&dyn rusqlite::ToSql> =
            chat_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        // Note: collect aborts on the first row-level error (e.g., NULL hours_ago from
        // malformed timestamps). A single bad row silently discards the entire result.
        // The Err arm logs and returns an empty/partial map per spec.
        let rows = stmt.query_map(params.as_slice(), |row| {
            let chat_id: String = row.get(0)?;
            let hours_ago: i64 = row.get(1)?;
            let cnt: u32 = row.get(2)?;
            Ok((chat_id, hours_ago, cnt))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
    });

    match query_result {
        Ok(triples) => {
            for (chat_id, hours_ago, cnt) in triples {
                if !(0..=23).contains(&hours_ago) {
                    continue;
                }
                let slot = 23 - hours_ago as usize;
                result.entry(chat_id).or_insert([0u32; 24])[slot] = cnt;
            }
        }
        Err(e) => {
            tracing::warn!("activity query failed: {}", e);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::{encode_braille, query_activity_24h};
    use crate::storage::db::Database;

    #[test]
    fn all_zero_returns_blank_braille() {
        let arr = [0u32; 24];
        let result = encode_braille(&arr);
        assert_eq!(result.chars().count(), 10);
        assert!(result.chars().all(|c| c == '⠀'));
    }

    #[test]
    fn single_spike_at_current_hour() {
        let mut arr = [0u32; 24];
        arr[23] = 100;
        let result = encode_braille(&arr);
        let chars: Vec<char> = result.chars().collect();
        assert_eq!(chars.len(), 10);
        assert_eq!(*chars.last().unwrap(), '⣿');
        for c in &chars[..9] {
            assert_eq!(*c, '⠀');
        }
    }

    #[test]
    fn uniform_distribution_all_same_level() {
        let mut arr = [0u32; 24];
        for i in 14..24 {
            arr[i] = 50;
        }
        let result = encode_braille(&arr);
        let chars: Vec<char> = result.chars().collect();
        let first = chars[0];
        assert!(chars.iter().all(|&c| c == first));
        assert_ne!(first, '⠀');
    }

    #[test]
    fn indices_outside_14_23_are_ignored() {
        let mut arr = [0u32; 24];
        arr[0] = 9999;
        arr[23] = 1;
        let chars: Vec<char> = encode_braille(&arr).chars().collect();
        assert_eq!(*chars.last().unwrap(), '⣿');
        for c in &chars[..9] {
            assert_eq!(*c, '⠀');
        }
    }

    #[test]
    fn empty_chat_ids_returns_empty_map() {
        let db = Database::open_in_memory().expect("in-memory db");
        let result = query_activity_24h(&db, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn query_with_no_matching_messages_returns_empty() {
        let db = Database::open_in_memory().expect("in-memory db");
        let result = query_activity_24h(&db, &["chat_nonexistent"]);
        assert!(result.is_empty());
    }

    #[test]
    fn query_returns_correct_slot_mapping() {
        let db = Database::open_in_memory().expect("in-memory db");

        // Insert a chat row first (messages has a FK to chats)
        db.conn
            .execute(
                "INSERT INTO chats (id, platform, name) VALUES ('chat_a', 'test', 'Chat A')",
                [],
            )
            .expect("insert chat_a");

        // Insert 3 messages in the current hour (hours_ago = 0, should go to slot 23)
        for i in 0..3 {
            db.conn.execute(
                "INSERT INTO messages (id, chat_id, platform, sender, content, timestamp, status)
                 VALUES (?1, 'chat_a', 'test', 'user', 'hi',
                         strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-' || ?2 || ' minutes'),
                         'delivered')",
                rusqlite::params![format!("msg_{}", i), i * 5],
            ).expect("insert message");
        }

        let result = query_activity_24h(&db, &["chat_a"]);

        assert!(result.contains_key("chat_a"), "chat_a should be in result");
        let arr = result["chat_a"];

        // Slot 23 = current hour (hours_ago = 0)
        assert_eq!(arr[23], 3, "slot 23 should have 3 messages");

        // All other slots should be 0
        for i in 0..23 {
            assert_eq!(arr[i], 0, "slot {} should be 0", i);
        }
    }

    #[test]
    fn query_multiple_chat_ids() {
        let db = Database::open_in_memory().expect("in-memory db");

        // Insert chat rows first
        db.conn
            .execute(
                "INSERT INTO chats (id, platform, name) VALUES ('chat_a', 'test', 'Chat A')",
                [],
            )
            .expect("insert chat_a");
        db.conn
            .execute(
                "INSERT INTO chats (id, platform, name) VALUES ('chat_b', 'test', 'Chat B')",
                [],
            )
            .expect("insert chat_b");

        // Insert 2 messages for chat_a and 1 for chat_b
        db.conn
            .execute(
                "INSERT INTO messages (id, chat_id, platform, sender, content, timestamp, status)
             VALUES ('a1', 'chat_a', 'test', 'user', 'hi',
                     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), 'delivered')",
                [],
            )
            .expect("insert a1");
        db.conn
            .execute(
                "INSERT INTO messages (id, chat_id, platform, sender, content, timestamp, status)
             VALUES ('a2', 'chat_a', 'test', 'user', 'hi',
                     strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-1 minutes'), 'delivered')",
                [],
            )
            .expect("insert a2");
        db.conn
            .execute(
                "INSERT INTO messages (id, chat_id, platform, sender, content, timestamp, status)
             VALUES ('b1', 'chat_b', 'test', 'user', 'hi',
                     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), 'delivered')",
                [],
            )
            .expect("insert b1");

        let result = query_activity_24h(&db, &["chat_a", "chat_b"]);

        assert_eq!(
            result.get("chat_a").map(|a| a[23]),
            Some(2),
            "chat_a should have 2 in slot 23"
        );
        assert_eq!(
            result.get("chat_b").map(|a| a[23]),
            Some(1),
            "chat_b should have 1 in slot 23"
        );
    }
}
