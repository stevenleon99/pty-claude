//! Launch specification for a session

use serde::{Deserialize, Serialize};

use crate::session::env::EffectiveEnvironment;
use crate::session::types::ProviderType;

/// Terminal dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TerminalSize {
    pub columns: u16,
    pub rows: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        TerminalSize {
            columns: 120,
            rows: 40,
        }
    }
}

/// Complete specification for launching a session process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub provider: ProviderType,
    pub executable: String,
    pub arguments: Vec<String>,
    pub effective_environment: EffectiveEnvironment,
    pub working_directory: String,
    pub terminal_size: TerminalSize,
}

/// Build a launch specification from session metadata and provider config.
pub fn build_launch_spec(
    provider: ProviderType,
    executable: String,
    arguments: Vec<String>,
    working_directory: String,
    terminal_size: TerminalSize,
    effective_environment: EffectiveEnvironment,
) -> LaunchSpec {
    LaunchSpec {
        provider,
        executable,
        arguments,
        effective_environment,
        working_directory,
        terminal_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_size_default() {
        let size = TerminalSize::default();
        assert_eq!(size.columns, 120);
        assert_eq!(size.rows, 40);
    }

    #[test]
    fn test_launch_spec_construction() {
        let spec = build_launch_spec(
            ProviderType::Codex,
            "codex".to_string(),
            vec!["--help".to_string()],
            "/tmp".to_string(),
            TerminalSize::default(),
            EffectiveEnvironment::default(),
        );
        assert_eq!(spec.provider, ProviderType::Codex);
        assert_eq!(spec.executable, "codex");
        assert_eq!(spec.arguments.len(), 1);
    }
}