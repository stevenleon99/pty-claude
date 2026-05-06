//! Environment configuration for sessions

use serde::{Deserialize, Serialize};

/// How to set up the environment for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvMode {
    /// Only base vars + explicit overrides + .env file
    Clean,
    /// Launch command through the configured login shell
    Shell,
    /// Capture env from login shell once, apply to child
    BootstrapFromShell,
}

impl EnvMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            EnvMode::Clean => "clean",
            EnvMode::Shell => "shell",
            EnvMode::BootstrapFromShell => "bootstrap_from_shell",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "clean" => Some(EnvMode::Clean),
            "shell" | "login_shell" | "login-shell" => Some(EnvMode::Shell),
            "bootstrap_from_shell" | "bootstrap" | "bootstrap-from-shell" => {
                Some(EnvMode::BootstrapFromShell)
            }
            _ => None,
        }
    }
}

impl Default for EnvMode {
    fn default() -> Self {
        EnvMode::Shell
    }
}

/// Per-session environment configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvConfig {
    pub mode: EnvMode,
    /// Per-session overrides (highest precedence).
    #[serde(default)]
    pub overrides: std::collections::HashMap<String, String>,
    /// Path to .env file. Relative paths resolved from workspace_root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_file_path: Option<String>,
}

impl Default for EnvConfig {
    fn default() -> Self {
        EnvConfig {
            mode: EnvMode::Shell,
            overrides: std::collections::HashMap::new(),
            env_file_path: None,
        }
    }
}

/// Source tag for each environment variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvSource {
    DaemonInherited,
    ServiceManager,
    BootstrapShell,
    EnvFile,
    ProviderConfig,
    SessionOverride,
}

impl EnvSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            EnvSource::DaemonInherited => "daemon_inherited",
            EnvSource::ServiceManager => "service_manager",
            EnvSource::BootstrapShell => "bootstrap_shell",
            EnvSource::EnvFile => "env_file",
            EnvSource::ProviderConfig => "provider_config",
            EnvSource::SessionOverride => "session_override",
        }
    }
}

impl Default for EnvSource {
    fn default() -> Self {
        EnvSource::DaemonInherited
    }
}

/// A single environment variable entry with its source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvEntry {
    pub key: String,
    pub value: String,
    pub source: EnvSource,
}

/// The resolved, ordered list of env vars for a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EffectiveEnvironment {
    pub entries: Vec<EnvEntry>,
    pub mode: EnvMode,
    /// Explicit overrides applied on top of inherited env.
    #[serde(default)]
    pub overrides: std::collections::HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap_shell_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap_warning: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_mode_parse() {
        assert_eq!(EnvMode::parse("clean"), Some(EnvMode::Clean));
        assert_eq!(EnvMode::parse("shell"), Some(EnvMode::Shell));
        assert_eq!(EnvMode::parse("login_shell"), Some(EnvMode::Shell));
        assert_eq!(EnvMode::parse("login-shell"), Some(EnvMode::Shell));
        assert_eq!(EnvMode::parse("bootstrap"), Some(EnvMode::BootstrapFromShell));
        assert_eq!(EnvMode::parse("bootstrap_from_shell"), Some(EnvMode::BootstrapFromShell));
        assert_eq!(EnvMode::parse("unknown"), None);
    }

    #[test]
    fn test_env_mode_roundtrip() {
        for mode in [EnvMode::Clean, EnvMode::Shell, EnvMode::BootstrapFromShell] {
            assert_eq!(EnvMode::parse(mode.as_str()), Some(mode));
        }
    }
}