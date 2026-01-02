//! RAG (Retrieval-Augmented Generation) module for Pangu
//!
//! Stores conversation history in `.pangu/history/` with UTC timestamps
//! and retrieves relevant past messages to augment the context.

mod retrieval;
mod store;

pub use retrieval::Retriever;
pub use store::{ConversationStore, StoredMessage};
