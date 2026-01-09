//! Context window management for preventing overflow
//!
//! This module provides utilities to manage the context window size
//! and prevent overflows that cause 500 errors from llama-server.

use super::message::{ChatMessage, Role};

/// Estimate token count from text (rough approximation)
/// Uses ~3 characters per token as a conservative estimate
fn estimate_tokens(text: &str) -> usize {
    // Rough estimate: ~3 chars per token (conservative to prevent overflow)
    // Real tokenizers often produce more tokens than 4 chars/token suggests
    (text.len() + 2) / 3
}

/// Estimate total tokens for a message
fn message_tokens(msg: &ChatMessage) -> usize {
    // Add overhead for message formatting (role, etc.)
    estimate_tokens(&msg.content) + 10
}

/// Truncate messages to fit within a context budget
///
/// Keeps:
/// - System prompt (always)
/// - Most recent messages (prioritized)
///
/// Removes oldest non-system messages when over budget.
pub fn truncate_to_context(
    messages: &[ChatMessage],
    max_context_tokens: usize,
    generation_budget: usize,
) -> Vec<ChatMessage> {
    // Reserve tokens for generation output
    let available_tokens = max_context_tokens.saturating_sub(generation_budget);

    // Use 85% of available context, leaving 15% safety margin
    let target_tokens = (available_tokens as f64 * 0.85) as usize;

    if target_tokens == 0 {
        // Return just system prompt if any
        return messages
            .iter()
            .filter(|m| matches!(m.role, Role::System))
            .cloned()
            .collect();
    }

    // Separate system messages (always keep) from others
    let system_messages: Vec<&ChatMessage> = messages
        .iter()
        .filter(|m| matches!(m.role, Role::System))
        .collect();

    let other_messages: Vec<&ChatMessage> = messages
        .iter()
        .filter(|m| !matches!(m.role, Role::System))
        .collect();

    // Calculate system message tokens
    let system_tokens: usize = system_messages.iter().map(|m| message_tokens(m)).sum();

    // Budget for non-system messages
    let other_budget = target_tokens.saturating_sub(system_tokens);

    // Work backwards from most recent, keeping messages that fit
    let mut kept_messages: Vec<&ChatMessage> = Vec::new();
    let mut used_tokens = 0;

    for msg in other_messages.iter().rev() {
        let msg_tokens = message_tokens(msg);
        if used_tokens + msg_tokens <= other_budget {
            kept_messages.push(msg);
            used_tokens += msg_tokens;
        } else {
            // Stop once we can't fit more
            break;
        }
    }

    // Reverse to get chronological order
    kept_messages.reverse();

    // Build final message list: system first, then kept messages
    let mut result: Vec<ChatMessage> = system_messages.into_iter().cloned().collect();
    result.extend(kept_messages.into_iter().cloned());

    // Log if we truncated
    let original_count = messages.len();
    let kept_count = result.len();
    if kept_count < original_count {
        tracing::info!(
            "Context truncated: kept {}/{} messages (~{} tokens, budget: {})",
            kept_count,
            original_count,
            system_tokens + used_tokens,
            target_tokens
        );
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_keeps_recent() {
        let messages = vec![
            ChatMessage::system("System prompt"),
            ChatMessage::user("Old message 1"),
            ChatMessage::assistant("Old response 1"),
            ChatMessage::user("Old message 2"),
            ChatMessage::assistant("Old response 2"),
            ChatMessage::user("Recent message"),
            ChatMessage::assistant("Recent response"),
        ];

        // Very small budget - should keep system + most recent
        let result = truncate_to_context(&messages, 200, 50);

        assert!(result.iter().any(|m| matches!(m.role, Role::System)));
        assert!(result.len() < messages.len());
    }

    #[test]
    fn test_truncate_keeps_all_if_fits() {
        let messages = vec![
            ChatMessage::system("System"),
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi"),
        ];

        // Large budget - should keep all
        let result = truncate_to_context(&messages, 10000, 1000);

        assert_eq!(result.len(), messages.len());
    }
}
