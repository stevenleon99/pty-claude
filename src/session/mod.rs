//! Session management types and abstractions

pub mod env;
pub mod env_file_parser;
pub mod env_resolver;
pub mod launch;
pub mod lifecycle;
pub mod output_buffer;
pub mod record;
pub mod registry;
pub mod runtime;
pub mod snapshot;
pub mod types;

// Unix-only PTY implementation
#[cfg(unix)]
pub mod posix_pty;

// Windows ConPTY implementation
#[cfg(windows)]
pub mod conpty;

// Placeholder modules for future phases
pub mod pty {
    //! PTY process management (Phase 2)

    use crate::session::launch::TerminalSize;

    /// Error type for PTY operations.
    #[derive(Debug, thiserror::Error)]
    pub enum PtyError {
        #[error("process already started")]
        AlreadyStarted,
        #[error("fork failed: {0}")]
        ForkFailed(#[source] std::io::Error),
        #[error("exec failed: {0}")]
        ExecFailed(String),
        #[error("I/O error: {0}")]
        Io(#[from] std::io::Error),
    }

    /// Result of a read operation from the PTY.
    #[derive(Debug, Clone)]
    pub struct ReadResult {
        pub data: Vec<u8>,
        pub closed: bool,
    }

    /// Trait for PTY process management.
    #[cfg(unix)]
    pub trait PtyProcess: Send + Sync {
        fn start(
            &mut self,
            spec: &crate::session::launch::LaunchSpec,
        ) -> Result<crate::session::types::ProcessId, PtyError>;
        fn write(&mut self, input: &[u8]) -> Result<(), PtyError>;
        fn read(&mut self, timeout_ms: u32) -> ReadResult;
        fn readable_fd(&self) -> Option<std::os::unix::io::RawFd>;
        fn resize(&mut self, size: TerminalSize) -> Result<(), PtyError>;
        fn poll_exit(&mut self) -> Option<i32>;
        fn terminate(&mut self) -> bool;
    }

    /// Non-Unix placeholder trait.
    #[cfg(not(unix))]
    pub trait PtyProcess: Send + Sync {
        fn start(
            &mut self,
            spec: &crate::session::launch::LaunchSpec,
        ) -> Result<crate::session::types::ProcessId, PtyError>;
        fn write(&mut self, input: &[u8]) -> Result<(), PtyError>;
        fn read(&mut self, timeout_ms: u32) -> ReadResult;
        fn resize(&mut self, size: TerminalSize) -> Result<(), PtyError>;
        fn poll_exit(&mut self) -> Option<i32>;
        fn terminate(&mut self) -> bool;
    }
}

pub mod terminal;

pub mod provider_config {
    //! Provider configuration

    use serde::{Deserialize, Serialize};

    use crate::session::types::ProviderType;

    /// Provider-specific configuration.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ProviderConfig {
        pub r#type: ProviderType,
        pub executable: String,
        #[serde(default)]
        pub default_args: Vec<String>,
        #[serde(default)]
        pub environment_overrides: std::collections::HashMap<String, String>,
    }

    impl ProviderConfig {
        pub fn default_for(provider: ProviderType) -> Self {
            match provider {
                ProviderType::Codex => ProviderConfig {
                    r#type: ProviderType::Codex,
                    executable: "codex".to_string(),
                    default_args: vec![],
                    environment_overrides: std::collections::HashMap::new(),
                },
                ProviderType::Claude => ProviderConfig {
                    r#type: ProviderType::Claude,
                    executable: "claude".to_string(),
                    default_args: vec![],
                    environment_overrides: std::collections::HashMap::new(),
                },
            }
        }
    }
}