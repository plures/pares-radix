//! Thread summarization — generates topic summaries for threads.
//!
//! When a thread reaches the configured message threshold, generates
//! a concise summary of the conversation. Used for thread list display
//! and cross-thread context.

use crate::model::ChatMessage;

/// Generate a simple extractive summary from thread messages.
/// This is the offline/fast version — no model call needed.
/// Returns the first user message topic + message count.
pub fn generate_quick_summary(messages: &[ChatMessage], max_len: usize) -> String {
    if messages.is_empty() {
        return "Empty thread".to_string();
    }

    // Find the first user message as the topic seed
    let first_user = messages
        .iter()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("(no user message)");

    // Truncate to max_len
    let truncated = if first_user.len() > max_len {
        format!("{}...", &first_user[..max_len.saturating_sub(3)])
    } else {
        first_user.to_string()
    };

    format!("{} ({} msgs)", truncated, messages.len())
}

/// Extract key topics from messages for display.
/// Returns up to `max_topics` topic keywords/phrases.
pub fn extract_topics(messages: &[ChatMessage], max_topics: usize) -> Vec<String> {
    // Simple keyword extraction — find unique first lines of user messages
    let mut topics: Vec<String> = messages
        .iter()
        .filter(|m| m.role == "user")
        .filter_map(|m| {
            let content = m.content.trim();
            // Skip commands
            if content.starts_with('/') {
                return None;
            }
            // Take first meaningful phrase (up to 30 chars)
            let phrase = content.split('\n').next().unwrap_or(content);
            let truncated = if phrase.len() > 30 {
                format!("{}...", &phrase[..27])
            } else {
                phrase.to_string()
            };
            Some(truncated)
        })
        .collect();

    topics.dedup();
    topics.truncate(max_topics);
    topics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_summary_empty() {
        let result = generate_quick_summary(&[], 50);
        assert_eq!(result, "Empty thread");
    }

    #[test]
    fn quick_summary_with_user_message() {
        let messages = vec![
            ChatMessage::user("How do I fix this bug?"),
            ChatMessage::assistant("Try restarting the service."),
        ];
        let result = generate_quick_summary(&messages, 50);
        assert!(result.contains("How do I fix this bug?"));
        assert!(result.contains("2 msgs"));
    }

    #[test]
    fn quick_summary_truncates_long_message() {
        let long_msg = "a".repeat(100);
        let messages = vec![ChatMessage::user(long_msg)];
        let result = generate_quick_summary(&messages, 20);
        assert!(result.contains("..."));
        // Total should be truncated portion + " (1 msgs)"
        assert!(result.len() < 40);
    }

    #[test]
    fn quick_summary_no_user_message() {
        let messages = vec![
            ChatMessage::system("System init"),
            ChatMessage::assistant("Hello!"),
        ];
        let result = generate_quick_summary(&messages, 50);
        assert!(result.contains("(no user message)"));
        assert!(result.contains("2 msgs"));
    }

    #[test]
    fn extract_topics_empty() {
        let topics = extract_topics(&[], 5);
        assert!(topics.is_empty());
    }

    #[test]
    fn extract_topics_skips_commands() {
        let messages = vec![
            ChatMessage::user("/thread new test"),
            ChatMessage::user("How do I deploy?"),
            ChatMessage::user("/help"),
        ];
        let topics = extract_topics(&messages, 5);
        assert_eq!(topics.len(), 1);
        assert_eq!(topics[0], "How do I deploy?");
    }

    #[test]
    fn extract_topics_truncates_long_phrases() {
        let messages = vec![ChatMessage::user(
            "This is a really long message that exceeds thirty characters easily",
        )];
        let topics = extract_topics(&messages, 5);
        assert_eq!(topics.len(), 1);
        assert!(topics[0].ends_with("..."));
        assert!(topics[0].len() <= 30);
    }

    #[test]
    fn extract_topics_limits_count() {
        let messages: Vec<_> = (0..10)
            .map(|i| ChatMessage::user(format!("Topic {i}")))
            .collect();
        let topics = extract_topics(&messages, 3);
        assert_eq!(topics.len(), 3);
    }

    #[test]
    fn extract_topics_deduplicates() {
        let messages = vec![
            ChatMessage::user("same message"),
            ChatMessage::user("same message"),
            ChatMessage::user("different"),
        ];
        let topics = extract_topics(&messages, 5);
        // dedup only removes *consecutive* duplicates
        assert_eq!(topics.len(), 2);
    }

    #[test]
    fn extract_topics_uses_first_line() {
        let messages = vec![ChatMessage::user("First line\nSecond line\nThird line")];
        let topics = extract_topics(&messages, 5);
        assert_eq!(topics[0], "First line");
    }

    #[test]
    fn extract_topics_skips_assistant_messages() {
        let messages = vec![
            ChatMessage::user("User question"),
            ChatMessage::assistant("Assistant response that is long"),
        ];
        let topics = extract_topics(&messages, 5);
        assert_eq!(topics.len(), 1);
        assert_eq!(topics[0], "User question");
    }
}
