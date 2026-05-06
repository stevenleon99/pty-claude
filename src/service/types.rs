//! Session category: interactive PTY vs managed log collection
//!
//! Defines the types of sessions supported and the service-level
//! session entry that bundles runtime, monitoring, and metadata.

use serde::{Deserialize, Serialize};

use crate::session::types::{ProcessId, SessionStatus};

/// Category of session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionCategory {
    Pty,
    ManagedLog,
}

impl Default for SessionCategory {
    fn default() -> Self {
        SessionCategory::Pty
    }
}

/// Log stream source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
}

/// Request to create a new PTY session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub provider: crate::session::types::ProviderType,
    pub working_directory: String,
    pub title: Option<String>,
    pub arguments: Vec<String>,
    pub env_file_path: Option<String>,
    pub terminal_size: Option<crate::session::launch::TerminalSize>,
}

/// Request to create a managed log session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLogSessionRequest {
    pub executable: String,
    pub arguments: Vec<String>,
    pub working_directory: String,
    pub title: Option<String>,
    pub env_file_path: Option<String>,
}

/// Summary of a session for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub provider: crate::session::types::ProviderType,
    pub status: SessionStatus,
    pub category: SessionCategory,
    pub pid: Option<ProcessId>,
    pub workspace_root: String,
}

/// Result of creating a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionResult {
    pub session_id: String,
    pub success: bool,
    pub error_message: Option<String>,
}

/// Result of sending input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputResult {
    pub success: bool,
    pub error_message: Option<String>,
}

/// Result of stopping a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopResult {
    pub success: bool,
    pub error_message: Option<String>,
}
