//! Session snapshot types

use serde::{Deserialize, Serialize};

use crate::session::types::{SessionMetadata, SessionStatus};

/// Supervision state of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SupervisionState {
    Active,
    Quiet,
    Stopped,
}

impl SupervisionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SupervisionState::Active => "active",
            SupervisionState::Quiet => "quiet",
            SupervisionState::Stopped => "stopped",
        }
    }
}

impl Default for SupervisionState {
    fn default() -> Self {
        SupervisionState::Quiet
    }
}

/// Attention state level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionState {
    None,
    Info,
    ActionRequired,
    Intervention,
}

impl AttentionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttentionState::None => "none",
            AttentionState::Info => "info",
            AttentionState::ActionRequired => "action_required",
            AttentionState::Intervention => "intervention",
        }
    }
}

impl Default for AttentionState {
    fn default() -> Self {
        AttentionState::None
    }
}

/// Reason for attention state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionReason {
    None,
    AwaitingInput,
    SessionError,
    WorkspaceChanged,
    GitStateChanged,
    ControllerChanged,
    SessionExitedCleanly,
}

impl AttentionReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttentionReason::None => "none",
            AttentionReason::AwaitingInput => "awaiting_input",
            AttentionReason::SessionError => "session_error",
            AttentionReason::WorkspaceChanged => "workspace_changed",
            AttentionReason::GitStateChanged => "git_state_changed",
            AttentionReason::ControllerChanged => "controller_changed",
            AttentionReason::SessionExitedCleanly => "session_exited_cleanly",
        }
    }
}

impl Default for AttentionReason {
    fn default() -> Self {
        AttentionReason::None
    }
}

/// How a session is interacting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionInteractionKind {
    Unknown,
    CompletedQuickly,
    RunningNonInteractive,
    InteractiveFullscreen,
    InteractiveLineMode,
}

impl Default for SessionInteractionKind {
    fn default() -> Self {
        SessionInteractionKind::Unknown
    }
}

/// Session activity state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionActivityState {
    Idle,
    MeaningfulOutput,
    CosmeticOutput,
    ExternalChange,
    Stopped,
}

impl Default for SessionActivityState {
    fn default() -> Self {
        SessionActivityState::Idle
    }
}

/// Summary of session mode.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionModeSummary {
    pub lifecycle_status: SessionStatus,
    pub interaction_kind: SessionInteractionKind,
    pub activity_state: SessionActivityState,
}

/// Summary of session attention.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionAttentionSummary {
    pub level: AttentionState,
    pub cause: AttentionReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since_unix_ms: Option<i64>,
    #[serde(default)]
    pub summary: String,
}

/// Terminal semantic change information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TerminalSemanticChange {
    pub kind: TerminalSemanticChangeKind,
    pub line_count_delta: i64,
}

/// Kind of terminal semantic change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalSemanticChangeKind {
    None,
    MeaningfulOutput,
    CosmeticOnly,
    ScreenClear,
}

impl Default for TerminalSemanticChangeKind {
    fn default() -> Self {
        TerminalSemanticChangeKind::None
    }
}

/// All signals for a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionSignals {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_raw_output_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_meaningful_output_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_output_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_file_change_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_git_change_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_controller_change_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attention_since_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pty_columns: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pty_rows: Option<u16>,
    #[serde(default)]
    pub current_sequence: u64,
    #[serde(default)]
    pub recent_file_change_count: usize,
    pub supervision_state: SupervisionState,
    pub attention_state: AttentionState,
    pub attention_reason: AttentionReason,
    pub interaction_kind: SessionInteractionKind,
    pub terminal_semantic_change: TerminalSemanticChange,
    #[serde(default)]
    pub git_dirty: bool,
    #[serde(default)]
    pub git_branch: String,
    #[serde(default)]
    pub git_modified_count: usize,
    #[serde(default)]
    pub git_staged_count: usize,
    #[serde(default)]
    pub git_untracked_count: usize,
    #[serde(default)]
    pub mode: SessionModeSummary,
    #[serde(default)]
    pub attention: SessionAttentionSummary,
}

/// Summary of a session node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionNodeSummary {
    pub session_id: String,
    pub lifecycle_status: SessionStatus,
    pub interaction_kind: SessionInteractionKind,
    pub attention_state: AttentionState,
    #[serde(default)]
    pub semantic_preview: String,
    #[serde(default)]
    pub recent_file_change_count: usize,
    #[serde(default)]
    pub git_dirty: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity_at_unix_ms: Option<i64>,
}

/// Git summary for a session.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GitSummary {
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub modified_count: usize,
    #[serde(default)]
    pub staged_count: usize,
    #[serde(default)]
    pub untracked_count: usize,
    #[serde(default)]
    pub modified_files: Vec<String>,
    #[serde(default)]
    pub staged_files: Vec<String>,
    #[serde(default)]
    pub untracked_files: Vec<String>,
}

/// Terminal screen snapshot (placeholder for Phase 2).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TerminalScreenSnapshot {
    pub lines: Vec<String>,
}

/// Complete session snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub metadata: SessionMetadata,
    #[serde(default)]
    pub current_sequence: u64,
    #[serde(default)]
    pub recent_terminal_tail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_screen: Option<TerminalScreenSnapshot>,
    #[serde(default)]
    pub signals: SessionSignals,
    #[serde(default)]
    pub node_summary: SessionNodeSummary,
    #[serde(default)]
    pub recent_file_changes: Vec<String>,
    #[serde(default)]
    pub git_summary: GitSummary,
}

/// Infer supervision state from session status and timing.
pub fn infer_supervision_state(
    status: SessionStatus,
    _last_output_at_unix_ms: Option<i64>,
    _now_unix_ms: i64,
) -> SupervisionState {
    match status {
        SessionStatus::Exited | SessionStatus::Error => SupervisionState::Stopped,
        SessionStatus::Running | SessionStatus::AwaitingInput => SupervisionState::Active,
        _ => SupervisionState::Quiet,
    }
}
