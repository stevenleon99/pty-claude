//! Core session types

use serde::{Deserialize, Serialize};
use std::fmt;

/// Type of AI provider driving the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    Codex,
    Claude,
}

impl ProviderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderType::Codex => "codex",
            ProviderType::Claude => "claude",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "codex" => Some(ProviderType::Codex),
            "claude" => Some(ProviderType::Claude),
            _ => None,
        }
    }
}

impl fmt::Display for ProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Current status of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SessionStatus {
    #[default]
    #[serde(rename = "Created")]
    Created,
    #[serde(rename = "Starting")]
    Starting,
    #[serde(rename = "Running")]
    Running,
    #[serde(rename = "AwaitingInput")]
    AwaitingInput,
    #[serde(rename = "Exited")]
    Exited,
    #[serde(rename = "Error")]
    Error,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Created => "Created",
            SessionStatus::Starting => "Starting",
            SessionStatus::Running => "Running",
            SessionStatus::AwaitingInput => "AwaitingInput",
            SessionStatus::Exited => "Exited",
            SessionStatus::Error => "Error",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Created" => Some(SessionStatus::Created),
            "Starting" => Some(SessionStatus::Starting),
            "Running" => Some(SessionStatus::Running),
            "AwaitingInput" => Some(SessionStatus::AwaitingInput),
            "Exited" => Some(SessionStatus::Exited),
            "Error" => Some(SessionStatus::Error),
            _ => None,
        }
    }
}

impl fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Kind of controller attached to a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerKind {
    None,
    Host,
    Remote,
}

impl ControllerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ControllerKind::None => "none",
            ControllerKind::Host => "host",
            ControllerKind::Remote => "remote",
        }
    }
}

impl fmt::Display for ControllerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Process ID type.
pub type ProcessId = i64;

/// Strongly-typed session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    /// Create a new session ID, returning None if the value is invalid.
    /// Valid characters: alphanumeric, underscore, hyphen.
    pub fn try_create(value: String) -> Option<Self> {
        if value.is_empty() {
            return None;
        }
        if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return None;
        }
        Some(SessionId(value))
    }

    /// Generate a new unique session ID.
    pub fn generate() -> Self {
        use rand::Rng;
        let id: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(16)
            .map(char::from)
            .collect();
        SessionId(format!("sess_{}", id.to_lowercase()))
    }

    /// Get the string value of the session ID.
    pub fn value(&self) -> &str {
        &self.0
    }

    /// Get the string value as a &str.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Session metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: SessionId,
    pub provider: ProviderType,
    pub workspace_root: String,
    pub title: String,
    pub status: SessionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub group_tags: Vec<String>,
}

/// Normalize a group tag: trim whitespace, lowercase.
pub fn normalize_group_tag(tag: &str) -> String {
    let trimmed = tag.trim();
    trimmed.to_lowercase()
}

/// Normalize a list of group tags: trim, lowercase, deduplicate.
pub fn normalize_group_tags(tags: &[String]) -> Vec<String> {
    let mut result: Vec<String> = Vec::with_capacity(tags.len());
    for tag in tags {
        let normalized = normalize_group_tag(tag);
        if normalized.is_empty() {
            continue;
        }
        if !result.contains(&normalized) {
            result.push(normalized);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_valid() {
        assert!(SessionId::try_create("abc123".to_string()).is_some());
        assert!(SessionId::try_create("my-session_id".to_string()).is_some());
        assert!(SessionId::try_create("a".to_string()).is_some());
    }

    #[test]
    fn test_session_id_invalid() {
        assert!(SessionId::try_create("".to_string()).is_none());
        assert!(SessionId::try_create("has space".to_string()).is_none());
        assert!(SessionId::try_create("has/slash".to_string()).is_none());
        assert!(SessionId::try_create("has.dot".to_string()).is_none());
    }

    #[test]
    fn test_provider_type_roundtrip() {
        assert_eq!(ProviderType::parse("codex"), Some(ProviderType::Codex));
        assert_eq!(ProviderType::parse("claude"), Some(ProviderType::Claude));
        assert_eq!(ProviderType::parse("unknown"), None);
    }

    #[test]
    fn test_session_status_roundtrip() {
        for status in [
            SessionStatus::Created,
            SessionStatus::Starting,
            SessionStatus::Running,
            SessionStatus::AwaitingInput,
            SessionStatus::Exited,
            SessionStatus::Error,
        ] {
            assert_eq!(SessionStatus::parse(status.as_str()), Some(status));
        }
    }

    #[test]
    fn test_normalize_group_tags() {
        let tags = vec![
            "  Foo  ".to_string(),
            "foo".to_string(),
            "  BAR  ".to_string(),
            "".to_string(),
            "  ".to_string(),
            "baz".to_string(),
        ];
        let result = normalize_group_tags(&tags);
        assert_eq!(result, vec!["foo", "bar", "baz"]);
    }
}
