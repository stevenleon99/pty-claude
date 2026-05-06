//! Session manager - central service for session lifecycle
//!
//! Manages creation, polling, input, and termination of both
//! PTY and managed-log sessions.

use std::collections::HashMap;

use crate::session::launch::TerminalSize;
use crate::session::pty::PtyProcess;
use crate::session::record::SessionRecord;
use crate::session::runtime::SessionRuntime;
use crate::session::snapshot::SessionSnapshot;
use crate::session::types::{ProcessId, SessionId, SessionMetadata, SessionStatus, ProviderType};
use crate::store::session_store::SessionStore;

use super::log_buffer::LogBuffer;
use super::types::{
    CreateSessionRequest, CreateSessionResult, InputResult,
    SessionCategory, SessionSummary, StopResult,
};

/// Entry for an active session in the manager.
pub struct SessionEntry<'a> {
    pub id: SessionId,
    pub category: SessionCategory,
    pub runtime: Option<SessionRuntime<'a>>,
    pub log_buffer: Option<LogBuffer>,
    pub pid: Option<ProcessId>,
}

/// The session manager service.
pub struct SessionManager<'store, 'pty> {
    sessions: HashMap<String, SessionEntry<'pty>>,
    session_store: &'store dyn SessionStore,
}

impl<'store, 'pty> SessionManager<'store, 'pty> {
    pub fn new(session_store: &'store dyn SessionStore) -> Self {
        SessionManager {
            sessions: HashMap::new(),
            session_store,
        }
    }

    /// Create a new PTY session.
    pub fn create_session(
        &mut self,
        request: CreateSessionRequest,
        pty_process: &'pty mut dyn PtyProcess,
    ) -> CreateSessionResult {
        let session_id = SessionId::generate();
        let id_str = session_id.as_str().to_string();

        let title = request.title.unwrap_or_else(|| {
            format!("{:?} session", request.provider)
        });

        let metadata = SessionMetadata {
            id: session_id.clone(),
            provider: request.provider,
            workspace_root: request.working_directory.clone(),
            title,
            status: SessionStatus::Created,
            conversation_id: None,
            group_tags: vec![],
        };

        let record = SessionRecord::new(metadata);

        let terminal_size = request.terminal_size.unwrap_or_default();

        let launch_spec = crate::session::launch::LaunchSpec {
            provider: request.provider,
            executable: crate::session::provider_config::ProviderConfig::default_for(request.provider).executable,
            arguments: request.arguments,
            effective_environment: crate::session::env::EffectiveEnvironment::default(),
            working_directory: request.working_directory,
            terminal_size,
        };

        let mut runtime = SessionRuntime::new(record, launch_spec, pty_process, 64 * 1024);

        if !runtime.start() {
            let error = runtime.start_error().clone().unwrap_or_else(|| "start failed".to_string());
            return CreateSessionResult {
                session_id: id_str,
                success: false,
                error_message: Some(error),
            };
        }

        let pid = runtime.pid();

        // Persist the session record
        let record = runtime.record().clone();
        let persisted = crate::store::session_store::PersistedSessionRecord {
            session_id: record.metadata().id.as_str().to_string(),
            provider: record.metadata().provider,
            workspace_root: record.metadata().workspace_root.clone(),
            title: record.metadata().title.clone(),
            status: record.metadata().status,
            conversation_id: record.metadata().conversation_id.clone(),
            group_tags: vec![],
            current_sequence: 0,
            recent_terminal_tail: String::new(),
        };
        let _ = self.session_store.upsert_session_record(&persisted);

        let entry = SessionEntry {
            id: session_id,
            category: SessionCategory::Pty,
            runtime: Some(runtime),
            log_buffer: None,
            pid,
        };

        self.sessions.insert(id_str.clone(), entry);

        CreateSessionResult {
            session_id: id_str,
            success: true,
            error_message: None,
        }
    }

    /// List all sessions.
    pub fn list_sessions(&self) -> Vec<SessionSummary> {
        self.sessions
            .values()
            .map(|entry| {
                let (status, pid, provider, workspace_root, title) = if let Some(ref runtime) = entry.runtime {
                    (
                        runtime.record().metadata().status,
                        runtime.pid(),
                        runtime.record().metadata().provider,
                        runtime.record().metadata().workspace_root.clone(),
                        runtime.record().metadata().title.clone(),
                    )
                } else {
                    (
                        SessionStatus::Exited,
                        entry.pid,
                        ProviderType::Codex,
                        String::new(),
                        String::new(),
                    )
                };

                SessionSummary {
                    id: entry.id.as_str().to_string(),
                    title,
                    provider,
                    status,
                    category: entry.category,
                    pid,
                    workspace_root,
                }
            })
            .collect()
    }

    /// Get a session snapshot.
    pub fn get_snapshot(&self, session_id: &str) -> Option<SessionSnapshot> {
        let entry = self.sessions.get(session_id)?;
        let runtime = entry.runtime.as_ref()?;
        Some(runtime.record().snapshot())
    }

    /// Send input to a session.
    pub fn send_input(&mut self, session_id: &str, input: &str) -> InputResult {
        let entry = match self.sessions.get_mut(session_id) {
            Some(e) => e,
            None => {
                return InputResult {
                    success: false,
                    error_message: Some("session not found".to_string()),
                }
            }
        };

        if let Some(ref mut runtime) = entry.runtime {
            if runtime.write_input(input) {
                InputResult {
                    success: true,
                    error_message: None,
                }
            } else {
                InputResult {
                    success: false,
                    error_message: Some("session not in interactive state".to_string()),
                }
            }
        } else {
            InputResult {
                success: false,
                error_message: Some("no runtime".to_string()),
            }
        }
    }

    /// Resize a session's terminal.
    pub fn resize_session(&mut self, session_id: &str, size: TerminalSize) -> InputResult {
        let entry = match self.sessions.get_mut(session_id) {
            Some(e) => e,
            None => {
                return InputResult {
                    success: false,
                    error_message: Some("session not found".to_string()),
                }
            }
        };

        if let Some(ref mut runtime) = entry.runtime {
            if runtime.resize_terminal(size) {
                InputResult {
                    success: true,
                    error_message: None,
                }
            } else {
                InputResult {
                    success: false,
                    error_message: Some("resize failed".to_string()),
                }
            }
        } else {
            InputResult {
                success: false,
                error_message: Some("no runtime".to_string()),
            }
        }
    }

    /// Stop a session.
    pub fn stop_session(&mut self, session_id: &str) -> StopResult {
        let entry = match self.sessions.get_mut(session_id) {
            Some(e) => e,
            None => {
                return StopResult {
                    success: false,
                    error_message: Some("session not found".to_string()),
                }
            }
        };

        if let Some(ref mut runtime) = entry.runtime {
            if runtime.shutdown() {
                StopResult {
                    success: true,
                    error_message: None,
                }
            } else {
                StopResult {
                    success: false,
                    error_message: Some("shutdown failed".to_string()),
                }
            }
        } else {
            StopResult {
                success: false,
                error_message: Some("no runtime".to_string()),
            }
        }
    }

    /// Poll a specific session for output.
    pub fn poll_session(&mut self, session_id: &str, timeout_ms: u32) -> bool {
        let entry = match self.sessions.get_mut(session_id) {
            Some(e) => e,
            None => return false,
        };

        if let Some(ref mut runtime) = entry.runtime {
            runtime.poll_once(timeout_ms);
            true
        } else {
            false
        }
    }

    /// Poll all sessions for output.
    pub fn poll_all(&mut self, timeout_ms: u32) {
        for entry in self.sessions.values_mut() {
            if let Some(ref mut runtime) = entry.runtime {
                runtime.poll_once(timeout_ms);
            }
        }
    }

    /// Remove a session from the manager.
    pub fn remove_session(&mut self, session_id: &str) -> bool {
        self.sessions.remove(session_id).is_some()
    }

    /// Number of active sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::launch::LaunchSpec as SessionLaunchSpec;
    use crate::session::pty::{PtyError, ReadResult};

    struct MockPty {
        started: bool,
    }

    impl MockPty {
        fn new() -> Self {
            MockPty { started: false }
        }
    }

    #[cfg(unix)]
    impl PtyProcess for MockPty {
        fn start(&mut self, _spec: &SessionLaunchSpec) -> Result<ProcessId, PtyError> {
            if self.started {
                return Err(PtyError::AlreadyStarted);
            }
            self.started = true;
            Ok(12345)
        }
        fn write(&mut self, _input: &[u8]) -> Result<(), PtyError> {
            Ok(())
        }
        fn read(&mut self, _timeout_ms: u32) -> ReadResult {
            ReadResult { data: vec![], closed: false }
        }
        fn readable_fd(&self) -> Option<std::os::unix::io::RawFd> {
            None
        }
        fn resize(&mut self, _size: TerminalSize) -> Result<(), PtyError> {
            Ok(())
        }
        fn poll_exit(&mut self) -> Option<i32> {
            None
        }
        fn terminate(&mut self) -> bool {
            true
        }
    }

    #[cfg(not(unix))]
    impl PtyProcess for MockPty {
        fn start(&mut self, _spec: &SessionLaunchSpec) -> Result<ProcessId, PtyError> {
            if self.started {
                return Err(PtyError::AlreadyStarted);
            }
            self.started = true;
            Ok(12345)
        }
        fn write(&mut self, _input: &[u8]) -> Result<(), PtyError> {
            Ok(())
        }
        fn read(&mut self, _timeout_ms: u32) -> ReadResult {
            ReadResult { data: vec![], closed: false }
        }
        fn resize(&mut self, _size: TerminalSize) -> Result<(), PtyError> {
            Ok(())
        }
        fn poll_exit(&mut self) -> Option<i32> {
            None
        }
        fn terminate(&mut self) -> bool {
            true
        }
    }

    struct MockSessionStore;

    impl SessionStore for MockSessionStore {
        fn load_sessions(&self) -> Vec<crate::store::session_store::PersistedSessionRecord> {
            vec![]
        }
        fn upsert_session_record(&self, _record: &crate::store::session_store::PersistedSessionRecord) -> bool {
            true
        }
        fn remove_session_record(&self, _session_id: &str) -> bool {
            true
        }
    }

    #[test]
    fn test_create_and_list_session() {
        let store = MockSessionStore;
        let mut manager = SessionManager::new(&store);

        let request = CreateSessionRequest {
            provider: ProviderType::Codex,
            working_directory: "/tmp".to_string(),
            title: Some("Test".to_string()),
            arguments: vec![],
            env_file_path: None,
            terminal_size: None,
        };

        let mut pty = MockPty::new();
        let result = manager.create_session(request, &mut pty);
        assert!(result.success);
        assert!(result.error_message.is_none());

        let sessions = manager.list_sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].status, SessionStatus::Running);
    }

    #[test]
    fn test_send_input() {
        let store = MockSessionStore;
        let mut manager = SessionManager::new(&store);

        let request = CreateSessionRequest {
            provider: ProviderType::Codex,
            working_directory: "/tmp".to_string(),
            title: None,
            arguments: vec![],
            env_file_path: None,
            terminal_size: None,
        };

        let mut pty = MockPty::new();
        let result = manager.create_session(request, &mut pty);
        let session_id = result.session_id;

        let input_result = manager.send_input(&session_id, "test\n");
        assert!(input_result.success);
    }

    #[test]
    fn test_stop_session() {
        let store = MockSessionStore;
        let mut manager = SessionManager::new(&store);

        let request = CreateSessionRequest {
            provider: ProviderType::Codex,
            working_directory: "/tmp".to_string(),
            title: None,
            arguments: vec![],
            env_file_path: None,
            terminal_size: None,
        };

        let mut pty = MockPty::new();
        let result = manager.create_session(request, &mut pty);
        let session_id = result.session_id;

        let stop_result = manager.stop_session(&session_id);
        assert!(stop_result.success);
    }

    #[test]
    fn test_session_not_found() {
        let store = MockSessionStore;
        let mut manager = SessionManager::new(&store);

        let input_result = manager.send_input("nonexistent", "test");
        assert!(!input_result.success);
    }
}
