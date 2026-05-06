//! Active session registry for WebSocket bridging
//!
//! Holds live PTY sessions that WebSocket handlers can interact with.
//! Uses tokio broadcast channels for output distribution to multiple clients.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;

use crate::session::launch::{LaunchSpec, TerminalSize};
use crate::session::pty::{PtyProcess, ReadResult};
use crate::session::types::ProcessId;

/// Output from a running session, broadcast to all connected clients.
#[derive(Debug, Clone)]
pub struct SessionOutput {
    pub data: Vec<u8>,
}

/// A running PTY session with its I/O channels.
pub struct RunningSession {
    /// The PTY process (boxed to allow dynamic dispatch).
    pty: Box<dyn PtyProcess>,
    /// Broadcast channel for output — multiple WebSocket clients can subscribe.
    output_tx: broadcast::Sender<SessionOutput>,
    /// Task handle for the background read loop.
    read_task: JoinHandle<()>,
    /// Process ID.
    pid: ProcessId,
    /// Current terminal size.
    size: TerminalSize,
    /// Flag indicating if the process has exited.
    exited: bool,
    /// Exit code (if exited).
    exit_code: Option<i32>,
}

impl RunningSession {
    /// Create a new running session.
    pub fn new(
        pty: Box<dyn PtyProcess>,
        pid: ProcessId,
        size: TerminalSize,
        output_tx: broadcast::Sender<SessionOutput>,
        read_task: JoinHandle<()>,
    ) -> Self {
        RunningSession {
            pty,
            output_tx,
            read_task,
            pid,
            size,
            exited: false,
            exit_code: None,
        }
    }

    /// Subscribe to output broadcasts.
    pub fn subscribe(&self) -> broadcast::Receiver<SessionOutput> {
        self.output_tx.subscribe()
    }

    /// Write input to the PTY.
    pub fn write(&mut self, input: &[u8]) -> bool {
        if self.exited {
            return false;
        }
        self.pty.write(input).is_ok()
    }

    /// Resize the terminal.
    pub fn resize(&mut self, size: TerminalSize) -> bool {
        if self.exited {
            return false;
        }
        if self.pty.resize(size).is_ok() {
            self.size = size;
            true
        } else {
            false
        }
    }

    /// Get current terminal size.
    pub fn size(&self) -> TerminalSize {
        self.size
    }

    /// Get process ID.
    pub fn pid(&self) -> ProcessId {
        self.pid
    }

    /// Check if exited.
    pub fn is_exited(&self) -> bool {
        self.exited
    }

    /// Get exit code.
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Poll for exit status.
    pub fn poll_exit(&mut self) -> Option<i32> {
        if self.exited {
            return self.exit_code;
        }
        let code = self.pty.poll_exit();
        if let Some(c) = code {
            self.exited = true;
            self.exit_code = Some(c);
        }
        code
    }

    /// Terminate the session.
    pub fn terminate(&mut self) {
        if !self.exited {
            self.pty.terminate();
            self.exited = true;
            self.exit_code = Some(-1);
        }
    }

    /// Read output from the PTY.
    pub fn read(&mut self, timeout_ms: u32) -> ReadResult {
        if self.exited {
            return ReadResult { data: Vec::new(), closed: true };
        }
        self.pty.read(timeout_ms)
    }

    /// Abort the read task.
    pub fn abort_read_task(&mut self) {
        self.read_task.abort();
    }
}

/// Registry of active sessions.
pub struct SessionRegistry {
    sessions: RwLock<HashMap<String, Arc<RwLock<RunningSession>>>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        SessionRegistry {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Create and start a new session.
    pub async fn create_session(
        &self,
        session_id: String,
        spec: &LaunchSpec,
        pty_factory: impl FnOnce() -> Box<dyn PtyProcess>,
    ) -> Result<ProcessId, String> {
        // Create PTY
        let mut pty = pty_factory();

        // Start the process
        let pid = pty.start(spec).map_err(|e| e.to_string())?;

        // Create broadcast channel for output
        let (output_tx, _) = broadcast::channel::<SessionOutput>(256);

        // Spawn background task to poll PTY output
        let output_tx_clone = output_tx.clone();
        let read_task = tokio::spawn(async move {
            // We need to poll the PTY for output continuously
            // Since PtyProcess::read takes &mut self, we need a different approach
            // For now, this is a placeholder — the actual polling happens in the WebSocket handler
            // In a production system, we'd use async I/O or a separate thread
            drop(output_tx_clone);
        });

        let session = RunningSession::new(pty, pid, spec.terminal_size, output_tx, read_task);

        // Store in registry
        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.clone(), Arc::new(RwLock::new(session)));

        Ok(pid)
    }

    /// Get a session by ID.
    pub async fn get_session(&self, session_id: &str) -> Option<Arc<RwLock<RunningSession>>> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// Remove a session.
    pub async fn remove_session(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(session_arc) = sessions.remove(session_id) {
            let mut session = session_arc.write().await;
            session.terminate();
            session.abort_read_task();
            true
        } else {
            false
        }
    }

    /// List all session IDs.
    pub async fn list_sessions(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }

    /// Count active sessions.
    pub async fn session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}