//! Session runtime - orchestrates PTY process, output buffer, terminal multiplexer
//!
//! This is the core session execution engine that ties together:
//! - SessionRecord (state tracking)
//! - PTY process (subprocess management)
//! - SessionOutputBuffer (output capture)
//! - TerminalMultiplexer (terminal state)

use crate::session::launch::TerminalSize;
use crate::session::output_buffer::SessionOutputBuffer;
use crate::session::pty::PtyProcess;
use crate::session::record::SessionRecord;
use crate::session::snapshot::GitSummary;
use crate::session::terminal::TerminalMultiplexer;
use crate::session::types::{ProcessId, SessionStatus};

use serde::{Deserialize, Serialize};

/// Launch specification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub provider: crate::session::types::ProviderType,
    pub executable: String,
    pub arguments: Vec<String>,
    pub effective_environment: crate::session::env::EffectiveEnvironment,
    pub working_directory: String,
    pub terminal_size: TerminalSize,
}

/// Session runtime managing the lifecycle of a terminal session.
pub struct SessionRuntime<'a> {
    record: SessionRecord,
    launch_spec: crate::session::launch::LaunchSpec,
    pty_process: &'a mut dyn PtyProcess,
    output_buffer: SessionOutputBuffer,
    terminal_multiplexer: TerminalMultiplexer,
    pid: Option<ProcessId>,
    start_error: Option<String>,
}

impl<'a> SessionRuntime<'a> {
    pub fn new(
        record: SessionRecord,
        launch_spec: crate::session::launch::LaunchSpec,
        pty_process: &'a mut dyn PtyProcess,
        output_buffer_capacity: usize,
    ) -> Self {
        let terminal_size = launch_spec.terminal_size;
        let mux = TerminalMultiplexer::new(terminal_size, 2000);

        SessionRuntime {
            record,
            launch_spec,
            pty_process,
            output_buffer: SessionOutputBuffer::new(output_buffer_capacity),
            terminal_multiplexer: mux,
            pid: None,
            start_error: None,
        }
    }

    pub fn record(&self) -> &SessionRecord {
        &self.record
    }

    pub fn launch_spec(&self) -> &crate::session::launch::LaunchSpec {
        &self.launch_spec
    }

    pub fn pid(&self) -> Option<ProcessId> {
        self.pid
    }

    pub fn start_error(&self) -> &Option<String> {
        &self.start_error
    }

    pub fn output_buffer(&self) -> &SessionOutputBuffer {
        &self.output_buffer
    }

    /// Start the session process.
    pub fn start(&mut self) -> bool {
        if !self.record.try_transition(SessionStatus::Starting) {
            return false;
        }

        self.start_error = None;

        match self.pty_process.start(&self.launch_spec_to_pty()) {
            Ok(pid) => {
                self.pid = Some(pid);
                self.record.try_transition(SessionStatus::Running)
            }
            Err(e) => {
                self.start_error = Some(e.to_string());
                self.record.try_transition(SessionStatus::Error);
                false
            }
        }
    }

    /// Write input to the process.
    pub fn write_input(&mut self, input: &str) -> bool {
        if !self.is_interactive_state() {
            return false;
        }
        self.pty_process.write(input.as_bytes()).is_ok()
    }

    /// Resize the terminal.
    pub fn resize_terminal(&mut self, size: TerminalSize) -> bool {
        if !self.is_interactive_state() {
            return false;
        }

        self.launch_spec.terminal_size = size;
        if self.pty_process.resize(size).is_err() {
            return false;
        }

        self.terminal_multiplexer.resize(size);
        true
    }

    /// Update a viewport for a specific viewer.
    pub fn update_viewport(&mut self, view_id: &str, size: TerminalSize) {
        self.terminal_multiplexer.update_viewport(view_id, size);
    }

    /// Remove a viewport.
    pub fn remove_viewport(&mut self, view_id: &str) {
        self.terminal_multiplexer.remove_viewport(view_id);
    }

    /// Get a viewport snapshot.
    pub fn viewport_snapshot(
        &self,
        view_id: &str,
    ) -> Option<crate::session::terminal::ViewportSnapshot> {
        self.terminal_multiplexer.viewport_snapshot(view_id)
    }

    /// Terminate the process.
    pub fn terminate(&mut self) -> bool {
        if self.pid.is_none() {
            return false;
        }
        self.pty_process.terminate()
    }

    /// Terminate and mark as exited.
    pub fn terminate_and_mark_exited(&mut self) -> bool {
        if self.pid.is_none() {
            return false;
        }

        if !self.pty_process.terminate() {
            return false;
        }

        self.pid = None;
        self.record.try_transition(SessionStatus::Exited)
    }

    /// Shutdown the session gracefully.
    pub fn shutdown(&mut self) -> bool {
        let status = self.record.metadata().status;
        if status == SessionStatus::Exited || status == SessionStatus::Error {
            self.pid = None;
            return true;
        }

        if self.pid.is_some() {
            if !self.pty_process.terminate() {
                return false;
            }
            self.pid = None;
        }

        match status {
            SessionStatus::Running | SessionStatus::AwaitingInput => {
                self.record.try_transition(SessionStatus::Exited)
            }
            SessionStatus::Starting => self.record.try_transition(SessionStatus::Error),
            SessionStatus::Created => true,
            _ => false,
        }
    }

    /// Mark the session as awaiting input.
    pub fn mark_awaiting_input(&mut self) -> bool {
        self.record.try_transition(SessionStatus::AwaitingInput)
    }

    /// Mark the session as running.
    pub fn mark_running(&mut self) -> bool {
        self.record.try_transition(SessionStatus::Running)
    }

    /// Handle process exit.
    pub fn handle_exit(&mut self, clean_exit: bool) -> bool {
        let transitioned = self
            .record
            .try_transition(if clean_exit {
                SessionStatus::Exited
            } else {
                SessionStatus::Error
            });

        if !transitioned {
            return false;
        }

        self.pid = None;
        true
    }

    /// Update git summary.
    pub fn update_git_summary(&mut self, summary: GitSummary) {
        self.record.set_git_summary(summary);
    }

    /// Update group tags.
    pub fn update_group_tags(&mut self, tags: Vec<String>) {
        self.record.set_group_tags(tags);
    }

    /// Update recent file changes.
    pub fn update_recent_file_changes(&mut self, changes: Vec<String>) {
        self.record.set_recent_file_changes(changes);
    }

    /// Poll the process once and process any output.
    pub fn poll_once(&mut self, read_timeout_ms: u32) {
        if self.pid.is_none() {
            return;
        }

        let mut timeout_remaining = read_timeout_ms;

        loop {
            let read_result = self.pty_process.read(timeout_remaining);
            timeout_remaining = 0;

            if !read_result.data.is_empty() {
                // Convert bytes to string (lossy for non-UTF8)
                let text = String::from_utf8_lossy(&read_result.data).to_string();
                self.output_buffer.append(text.clone());
                self.terminal_multiplexer.append(&text);

                self.record
                    .set_current_sequence(self.output_buffer.next_sequence() - 1);

                let tail = self.output_buffer.tail(64 * 1024);
                self.record.set_recent_terminal_tail(tail.data);
            }

            if read_result.closed {
                let exit_code = self.pty_process.poll_exit();
                if exit_code.is_some() {
                    let clean = exit_code == Some(0);
                    self.handle_exit(clean);
                }
                return;
            }

            if read_result.data.is_empty() {
                break;
            }
        }

        // Check for exit
        let exit_code = self.pty_process.poll_exit();
        if exit_code.is_some() {
            let clean = exit_code == Some(0);
            self.handle_exit(clean);
        }
    }

    fn is_interactive_state(&self) -> bool {
        let status = self.record.metadata().status;
        status == SessionStatus::Running || status == SessionStatus::AwaitingInput
    }

    /// Convert our LaunchSpec to the session::launch::LaunchSpec format
    fn launch_spec_to_pty(&self) -> crate::session::launch::LaunchSpec {
        crate::session::launch::LaunchSpec {
            provider: self.launch_spec.provider,
            executable: self.launch_spec.executable.clone(),
            arguments: self.launch_spec.arguments.clone(),
            effective_environment: self.launch_spec.effective_environment.clone(),
            working_directory: self.launch_spec.working_directory.clone(),
            terminal_size: self.launch_spec.terminal_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::launch::LaunchSpec as SessionLaunchSpec;
    use crate::session::pty::{PtyError, ReadResult};
    use crate::session::types::{ProviderType, SessionId, SessionMetadata};

    /// Mock PTY process for testing
    struct MockPty {
        started: bool,
        output_queue: Vec<String>,
    }

    impl MockPty {
        fn new() -> Self {
            MockPty {
                started: false,
                output_queue: Vec::new(),
            }
        }

        fn with_output(mut self, output: Vec<&str>) -> Self {
            self.output_queue = output.iter().map(|s| s.to_string()).collect();
            self
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
            if let Some(data) = self.output_queue.pop() {
                ReadResult {
                    data: data.into_bytes(),
                    closed: false,
                }
            } else {
                ReadResult {
                    data: vec![],
                    closed: false,
                }
            }
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
            if let Some(data) = self.output_queue.pop() {
                ReadResult {
                    data: data.into_bytes(),
                    closed: false,
                }
            } else {
                ReadResult {
                    data: vec![],
                    closed: false,
                }
            }
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

    fn test_launch_spec() -> SessionLaunchSpec {
        SessionLaunchSpec {
            provider: ProviderType::Codex,
            executable: "echo".to_string(),
            arguments: vec!["hello".to_string()],
            effective_environment: crate::session::env::EffectiveEnvironment::default(),
            working_directory: "/tmp".to_string(),
            terminal_size: TerminalSize::default(),
        }
    }

    #[test]
    fn test_runtime_start() {
        let record = SessionRecord::new(test_metadata());
        let spec = test_launch_spec();
        let mut pty = MockPty::new();
        let mut runtime = SessionRuntime::new(record, spec, &mut pty, 64 * 1024);

        assert!(runtime.start());
        assert_eq!(runtime.pid(), Some(12345));
        assert_eq!(runtime.record().metadata().status, SessionStatus::Running);
    }

    #[test]
    fn test_runtime_write_when_not_started() {
        let record = SessionRecord::new(test_metadata());
        let spec = test_launch_spec();
        let mut pty = MockPty::new();
        let mut runtime = SessionRuntime::new(record, spec, &mut pty, 64 * 1024);

        // Not started yet, write should fail
        assert!(!runtime.write_input("test"));
    }

    #[test]
    fn test_runtime_shutdown() {
        let record = SessionRecord::new(test_metadata());
        let spec = test_launch_spec();
        let mut pty = MockPty::new();
        let mut runtime = SessionRuntime::new(record, spec, &mut pty, 64 * 1024);

        runtime.start();
        assert!(runtime.shutdown());
        assert_eq!(runtime.record().metadata().status, SessionStatus::Exited);
    }
}
