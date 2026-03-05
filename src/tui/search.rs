use crate::core::types::UnifiedChat;

/// Returns the match span length (lower = tighter match = better).
/// All chars of `query` must appear in order in `text` (case-insensitive).
/// Returns None if no match.
pub fn fuzzy_score(query: &str, text: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    let query_chars: Vec<char> = query.to_lowercase().chars().collect();
    let text_chars: Vec<char> = text.to_lowercase().chars().collect();
    let mut qi = 0;
    let mut first_match: Option<usize> = None;
    let mut last_match = 0;
    for (ti, &tc) in text_chars.iter().enumerate() {
        if tc == query_chars[qi] {
            if first_match.is_none() {
                first_match = Some(ti);
            }
            last_match = ti;
            qi += 1;
            if qi == query_chars.len() {
                return Some(last_match - first_match.unwrap());
            }
        }
    }
    None
}

/// Returns up to `limit` chat indices from `chats`, sorted by fuzzy score (best first).
/// Returns empty vec when query is empty.
pub fn top_fuzzy_matches(query: &str, chats: &[UnifiedChat], limit: usize) -> Vec<usize> {
    if query.is_empty() {
        return vec![];
    }
    let mut scored: Vec<(usize, usize)> = chats
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let name = c.display_name.as_deref().unwrap_or(&c.name);
            fuzzy_score(query, name).map(|s| (i, s))
        })
        .collect();
    scored.sort_by_key(|&(_, s)| s);
    scored.into_iter().take(limit).map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_score_exact() {
        assert_eq!(fuzzy_score("xin", "xin wei"), Some(2));
    }

    #[test]
    fn test_fuzzy_score_scattered() {
        // 'x' at 0, 'w' at 4 → span 4
        assert_eq!(fuzzy_score("xw", "xin wei"), Some(4));
    }

    #[test]
    fn test_fuzzy_score_no_match() {
        assert_eq!(fuzzy_score("zzz", "xin wei"), None);
    }

    #[test]
    fn test_fuzzy_score_empty_query() {
        assert_eq!(fuzzy_score("", "anything"), Some(0));
    }

    #[test]
    fn test_fuzzy_score_case_insensitive() {
        assert_eq!(fuzzy_score("XIN", "xin wei"), Some(2));
    }

    #[test]
    fn test_top_fuzzy_matches_empty_query() {
        let chats = vec![make_chat("alice"), make_chat("bob")];
        assert!(top_fuzzy_matches("", &chats, 5).is_empty());
    }

    #[test]
    fn test_top_fuzzy_matches_limit() {
        let chats = vec![
            make_chat("alice"),
            make_chat("alan"),
            make_chat("alex"),
            make_chat("albert"),
            make_chat("alvin"),
            make_chat("aldous"),
        ];
        let results = top_fuzzy_matches("al", &chats, 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_top_fuzzy_matches_sorted_by_score() {
        // "xin" scores 2 on "xin wei", higher span on "xi_n_long"
        let chats = vec![make_chat("xi_n_long"), make_chat("xin wei")];
        let results = top_fuzzy_matches("xin", &chats, 5);
        // "xin wei" (index 1) should rank first (tighter match)
        assert_eq!(results[0], 1);
    }

    fn make_chat(name: &str) -> UnifiedChat {
        UnifiedChat {
            id: name.to_string(),
            name: name.to_string(),
            display_name: None,
            platform: crate::core::types::Platform::Mock,
            last_message: None,
            unread_count: 0,
            is_group: false,
            is_pinned: false,
            is_newsletter: false,
        }
    }
}
