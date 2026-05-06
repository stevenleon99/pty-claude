//! Session record - mutable state container for a session

use crate::session::lifecycle::SessionLifecycle;
use crate::session::snapshot::{
    AttentionReason, AttentionState, GitSummary, SessionInteractionKind, SessionNodeSummary,
    SessionSignals, SessionSnapshot, SupervisionState, TerminalSemanticChange,
};
use crate::session::types::{SessionMetadata, SessionStatus};

/// Mutable record tracking a session's state.
#[derive(Debug, Clone)]
pub struct SessionRecord {
    metadata: SessionMetadata,
    lifecycle: SessionLifecycle,
    current_sequence: u64,
    recent_terminal_tail: String,
    terminal_screen: Option<crate::session::snapshot::TerminalScreenSnapshot>,
    terminal_semantic_change: TerminalSemanticChange,
    recent_file_changes: Vec<String>,
    git_summary: GitSummary,
}

impl SessionRecord {
    pub fn new(metadata: SessionMetadata) -> Self {
        SessionRecord {
            metadata,
            lifecycle: SessionLifecycle::new(),
            current_sequence: 0,
            recent_terminal_tail: String::new(),
            terminal_screen: None,
            terminal_semantic_change: TerminalSemanticChange::default(),
            recent_file_changes: Vec::new(),
            git_summary: GitSummary::default(),
        }
    }

    pub fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    pub fn lifecycle(&self) -> &SessionLifecycle {
        &self.lifecycle
    }

    /// Attempt to transition the session to a new status.
    /// Returns true if the transition was valid and applied.
    pub fn try_transition(&mut self, next_status: SessionStatus) -> bool {
        if !self.lifecycle.try_transition(next_status) {
            return false;
        }
        self.metadata.status = self.lifecycle.state();
        true
    }

    pub fn set_current_sequence(&mut self, sequence: u64) {
        self.current_sequence = sequence;
    }

    pub fn set_recent_terminal_tail(&mut self, tail: String) {
        self.recent_terminal_tail = tail;
    }

    pub fn set_terminal_screen(
        &mut self,
        screen: crate::session::snapshot::TerminalScreenSnapshot,
    ) {
        self.terminal_screen = Some(screen);
    }

    pub fn set_last_terminal_semantic_change(&mut self, change: TerminalSemanticChange) {
        self.terminal_semantic_change = change;
    }

    pub fn set_recent_file_changes(&mut self, changes: Vec<String>) {
        self.recent_file_changes = changes;
    }

    pub fn set_git_summary(&mut self, summary: GitSummary) {
        self.git_summary = summary;
    }

    pub fn set_group_tags(&mut self, tags: Vec<String>) {
        self.metadata.group_tags = tags;
    }

    /// Create a snapshot of the current session state.
    pub fn snapshot(&self) -> SessionSnapshot {
        let git_dirty = !self.git_summary.modified_files.is_empty()
            || !self.git_summary.staged_files.is_empty()
            || !self.git_summary.untracked_files.is_empty();

        SessionSnapshot {
            metadata: self.metadata.clone(),
            current_sequence: self.current_sequence,
            recent_terminal_tail: self.recent_terminal_tail.clone(),
            terminal_screen: self.terminal_screen.clone(),
            signals: SessionSignals {
                current_sequence: self.current_sequence,
                recent_file_change_count: self.recent_file_changes.len(),
                attention_state: AttentionState::None,
                attention_reason: AttentionReason::None,
                interaction_kind: SessionInteractionKind::Unknown,
                terminal_semantic_change: self.terminal_semantic_change.clone(),
                git_dirty,
                git_branch: self.git_summary.branch.clone(),
                supervision_state: SupervisionState::Quiet,
                ..Default::default()
            },
            node_summary: SessionNodeSummary {
                session_id: self.metadata.id.value().to_string(),
                lifecycle_status: self.metadata.status,
                interaction_kind: SessionInteractionKind::Unknown,
                attention_state: AttentionState::None,
                semantic_preview: String::new(),
                recent_file_change_count: self.recent_file_changes.len(),
                git_dirty,
                ..Default::default()
            },
            recent_file_changes: self.recent_file_changes.clone(),
            git_summary: self.git_summary.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{ProviderType, SessionId};

    fn test_metadata() -> SessionMetadata {
        SessionMetadata {
            id: SessionId::try_create("test_session".to_string()).unwrap(),
            provider: ProviderType::Codex,
            workspace_root: "/tmp".to_string(),
            title: "Test".to_string(),
            status: SessionStatus::Created,
            conversation_id: None,
            group_tags: vec![],
        }
    }

    #[test]
    fn test_record_transition() {
        let mut record = SessionRecord::new(test_metadata());
        assert_eq!(record.metadata().status, SessionStatus::Created);

        assert!(record.try_transition(SessionStatus::Starting));
        assert_eq!(record.metadata().status, SessionStatus::Starting);

        assert!(record.try_transition(SessionStatus::Running));
        assert_eq!(record.metadata().status, SessionStatus::Running);

        // Invalid: Running -> Starting
        assert!(!record.try_transition(SessionStatus::Starting));
        assert_eq!(record.metadata().status, SessionStatus::Running);
    }

    #[test]
    fn test_snapshot_basic() {
        let mut record = SessionRecord::new(test_metadata());
        record.try_transition(SessionStatus::Starting);
        record.try_transition(SessionStatus::Running);
        record.set_current_sequence(42);
        record.set_git_summary(GitSummary {
            branch: "main".to_string(),
            modified_count: 3,
            modified_files: vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()],
            ..Default::default()
        });

        let snap = record.snapshot();
        assert_eq!(snap.current_sequence, 42);
        assert_eq!(snap.metadata.status, SessionStatus::Running);
        assert!(snap.signals.git_dirty);
        assert_eq!(snap.signals.git_branch, "main");
        assert_eq!(snap.node_summary.recent_file_change_count, 0);
    }
}
