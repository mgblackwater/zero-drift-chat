use crate::ai::providers::{ContextMessage, MessageRole};

pub const SYSTEM_PROMPT: &str =
    "You are a chat autocomplete assistant. Complete the user's message naturally \
     based on the conversation context. Reply with ONLY the completion text — \
     no explanation, no quotes, no prefix.";

pub struct RawMessage {
    pub is_outgoing: bool,
    pub text: String,
}

/// Assemble context from optional summary + last N messages.
pub fn build_context(
    messages: &[RawMessage],
    summary: Option<&str>,
    last_n: usize,
) -> Vec<ContextMessage> {
    let mut ctx: Vec<ContextMessage> = Vec::new();

    if let Some(s) = summary {
        ctx.push(ContextMessage {
            role: MessageRole::Assistant,
            content: format!("[Conversation summary]: {}", s),
        });
    }

    let start = messages.len().saturating_sub(last_n);
    for msg in &messages[start..] {
        ctx.push(ContextMessage {
            role: if msg.is_outgoing { MessageRole::User } else { MessageRole::Assistant },
            content: msg.text.clone(),
        });
    }

    ctx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::providers::MessageRole;

    #[test]
    fn builds_context_from_messages() {
        let messages = vec![
            RawMessage { is_outgoing: true,  text: "hey".to_string() },
            RawMessage { is_outgoing: false, text: "yo".to_string() },
        ];
        let ctx = build_context(&messages, None, 10);
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].to_chat_line(), "[You]: hey");
        assert_eq!(ctx[1].to_chat_line(), "[Them]: yo");
    }

    #[test]
    fn limits_to_last_n() {
        let messages: Vec<RawMessage> = (0..20).map(|i| RawMessage {
            is_outgoing: i % 2 == 0,
            text: format!("msg {}", i),
        }).collect();
        let ctx = build_context(&messages, None, 5);
        assert_eq!(ctx.len(), 5);
        assert_eq!(ctx.last().unwrap().to_chat_line(), "[Them]: msg 19");
    }

    #[test]
    fn prepends_summary_as_first_message() {
        let messages = vec![
            RawMessage { is_outgoing: true, text: "hi".to_string() },
        ];
        let ctx = build_context(&messages, Some("Chat about cats"), 10);
        assert_eq!(ctx.len(), 2);
        assert!(matches!(ctx[0].role, MessageRole::Assistant));
        assert!(ctx[0].content.contains("cats"));
    }
}
