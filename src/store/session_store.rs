//! Session persistence store trait

use serde::{Deserialize, Serialize};

use crate::session::types::{ProviderType, SessionStatus};

/// A persisted session record for recovery across restarts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedSessionRecord {
    pub session_id: String,
    pub provider: ProviderType,
    pub workspace_root: String,
    pub title: String,
    pub status: SessionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub group_tags: Vec<String>,
    #[serde(default)]
    pub current_sequence: u64,
    #[serde(default)]
    pub recent_terminal_tail: String,
}

/// Trait for session record persistence.
pub trait SessionStore: Send + Sync {
    fn load_sessions(&self) -> Vec<PersistedSessionRecord>;
    fn upsert_session_record(&self, record: &PersistedSessionRecord) -> bool;
    fn remove_session_record(&self, session_id: &str) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persisted_record_serde_roundtrip() {
        let record = PersistedSessionRecord {
            session_id: "test_123".to_string(),
            provider: ProviderType::Codex,
            workspace_root: "/tmp".to_string(),
            title: "Test Session".to_string(),
            status: SessionStatus::Running,
            conversation_id: Some("conv_456".to_string()),
            group_tags: vec!["tag1".to_string()],
            current_sequence: 42,
            recent_terminal_tail: "some output".to_string(),
        };

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: PersistedSessionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }
}