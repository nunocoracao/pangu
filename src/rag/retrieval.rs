//! Simple retrieval system for RAG
//!
//! Uses a BM25-like scoring algorithm for keyword-based retrieval.

use std::collections::{HashMap, HashSet};

use super::StoredMessage;
use crate::model::ChatMessage;

/// Simple BM25-like retriever
pub struct Retriever {
    /// BM25 k1 parameter (term frequency saturation)
    k1: f64,
    /// BM25 b parameter (document length normalization)
    b: f64,
}

impl Default for Retriever {
    fn default() -> Self {
        Self::new()
    }
}

impl Retriever {
    /// Create a new retriever with default parameters
    pub fn new() -> Self {
        Self {
            k1: 1.5,
            b: 0.75,
        }
    }

    /// Tokenize text into words
    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty() && s.len() > 2) // Skip short words
            .map(String::from)
            .collect()
    }

    /// Calculate IDF (Inverse Document Frequency) for each term
    fn calculate_idf(documents: &[Vec<String>]) -> HashMap<String, f64> {
        let n = documents.len() as f64;
        let mut doc_freq: HashMap<String, usize> = HashMap::new();

        for doc in documents {
            let unique_terms: HashSet<_> = doc.iter().collect();
            for term in unique_terms {
                *doc_freq.entry(term.clone()).or_insert(0) += 1;
            }
        }

        doc_freq
            .into_iter()
            .map(|(term, df)| {
                // BM25 IDF formula
                let idf = ((n - df as f64 + 0.5) / (df as f64 + 0.5) + 1.0).ln();
                (term, idf.max(0.0))
            })
            .collect()
    }

    /// Calculate BM25 score for a document given a query
    fn bm25_score(
        &self,
        query_terms: &[String],
        doc_terms: &[String],
        idf: &HashMap<String, f64>,
        avg_doc_len: f64,
    ) -> f64 {
        let doc_len = doc_terms.len() as f64;

        // Count term frequencies in document
        let mut term_freq: HashMap<&String, usize> = HashMap::new();
        for term in doc_terms {
            *term_freq.entry(term).or_insert(0) += 1;
        }

        let mut score = 0.0;
        for term in query_terms {
            if let Some(&tf) = term_freq.get(term) {
                let idf_score = idf.get(term).copied().unwrap_or(0.0);
                let tf = tf as f64;

                // BM25 scoring formula
                let numerator = tf * (self.k1 + 1.0);
                let denominator = tf + self.k1 * (1.0 - self.b + self.b * (doc_len / avg_doc_len));

                score += idf_score * (numerator / denominator);
            }
        }

        score
    }

    /// Retrieve the top-k most relevant messages for a query
    pub fn retrieve(
        &self,
        query: &str,
        messages: &[StoredMessage],
        top_k: usize,
    ) -> Vec<StoredMessage> {
        if messages.is_empty() {
            return Vec::new();
        }

        // Tokenize all messages
        let doc_tokens: Vec<Vec<String>> = messages
            .iter()
            .map(|m| Self::tokenize(&m.content))
            .collect();

        // Calculate average document length
        let total_tokens: usize = doc_tokens.iter().map(|d| d.len()).sum();
        let avg_doc_len = total_tokens as f64 / doc_tokens.len() as f64;

        // Calculate IDF
        let idf = Self::calculate_idf(&doc_tokens);

        // Tokenize query
        let query_terms = Self::tokenize(query);

        if query_terms.is_empty() {
            return Vec::new();
        }

        // Score all documents
        let mut scored: Vec<(usize, f64)> = doc_tokens
            .iter()
            .enumerate()
            .map(|(i, doc)| {
                let score = self.bm25_score(&query_terms, doc, &idf, avg_doc_len);
                (i, score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top-k
        scored
            .into_iter()
            .take(top_k)
            .map(|(i, _)| messages[i].clone())
            .collect()
    }

    /// Retrieve relevant messages and combine with recent messages
    /// Returns (retrieved_messages, recent_messages)
    pub fn retrieve_with_context(
        &self,
        query: &str,
        all_history: &[StoredMessage],
        current_messages: &[ChatMessage],
        max_rag_messages: usize,
        max_recent_messages: usize,
    ) -> (Vec<ChatMessage>, Vec<ChatMessage>) {
        // Get recent messages (excluding system)
        let recent: Vec<ChatMessage> = current_messages
            .iter()
            .filter(|m| m.role != crate::model::Role::System)
            .cloned()
            .collect();

        let recent: Vec<ChatMessage> = if recent.len() > max_recent_messages {
            recent[recent.len() - max_recent_messages..].to_vec()
        } else {
            recent
        };

        // Retrieve from history (exclude current conversation)
        let retrieved = self.retrieve(query, all_history, max_rag_messages);

        let rag_messages: Vec<ChatMessage> = retrieved
            .into_iter()
            .map(|m| m.to_chat_message())
            .collect();

        (rag_messages, recent)
    }

    /// Build context messages for the LLM
    /// Combines RAG-retrieved messages with recent messages
    pub fn build_context(
        &self,
        query: &str,
        all_history: &[StoredMessage],
        current_messages: &[ChatMessage],
        system_prompt: Option<&str>,
        max_rag_messages: usize,
        max_recent_messages: usize,
    ) -> (Vec<ChatMessage>, usize) {
        let (rag_messages, recent_messages) = self.retrieve_with_context(
            query,
            all_history,
            current_messages,
            max_rag_messages,
            max_recent_messages,
        );

        let rag_count = rag_messages.len();
        let mut context = Vec::new();

        // Build combined system prompt with RAG context
        // (Only ONE system message is allowed by most LLM servers)
        let combined_system = if !rag_messages.is_empty() {
            let rag_context = rag_messages
                .iter()
                .map(|m| format!("[{}]: {}",
                    match m.role {
                        crate::model::Role::User => "User",
                        crate::model::Role::Assistant => "Assistant",
                        crate::model::Role::Tool => "Tool",
                        crate::model::Role::System => "System",
                    },
                    m.content
                ))
                .collect::<Vec<_>>()
                .join("\n\n");

            if let Some(prompt) = system_prompt {
                format!(
                    "{}\n\n## Relevant Context from Previous Conversations\n\n{}",
                    prompt, rag_context
                )
            } else {
                format!(
                    "## Relevant Context from Previous Conversations\n\n{}",
                    rag_context
                )
            }
        } else {
            system_prompt.unwrap_or("").to_string()
        };

        // Add single combined system message
        if !combined_system.is_empty() {
            context.push(ChatMessage::system(&combined_system));
        }

        // Add recent messages
        context.extend(recent_messages);

        (context, rag_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_message(role: &str, content: &str) -> StoredMessage {
        StoredMessage {
            timestamp: Utc::now(),
            role: role.to_string(),
            content: content.to_string(),
            conversation_id: "test".to_string(),
        }
    }

    #[test]
    fn test_tokenize() {
        let tokens = Retriever::tokenize("Hello, world! This is a test.");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"this".to_string()));
        assert!(tokens.contains(&"test".to_string()));
        // Short words should be filtered
        assert!(!tokens.contains(&"is".to_string()));
        assert!(!tokens.contains(&"a".to_string()));
    }

    #[test]
    fn test_retrieve_basic() {
        let retriever = Retriever::new();

        let messages = vec![
            make_message("user", "How do I implement a binary search tree in Rust?"),
            make_message("assistant", "Here's how to implement a binary search tree..."),
            make_message("user", "What's the weather like today?"),
            make_message("assistant", "I don't have access to weather information."),
            make_message("user", "Can you explain recursion in programming?"),
        ];

        // Query about trees should retrieve tree-related messages
        let results = retriever.retrieve("binary tree implementation", &messages, 2);
        assert!(!results.is_empty());
        assert!(results[0].content.contains("binary search tree"));
    }

    #[test]
    fn test_retrieve_empty() {
        let retriever = Retriever::new();
        let results = retriever.retrieve("hello", &[], 5);
        assert!(results.is_empty());
    }
}
