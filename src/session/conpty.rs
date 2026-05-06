//! Windows ConPTY implementation using portable-pty
//!
//! Uses portable-pty for correct ConPTY creation and child process attachment.
//! A background thread reads PTY output and sends it through a channel
//! for non-blocking consumption by the WebSocket output pump.
//!
//! Key design:
//! - Exit detection uses the child process handle (try_wait), NOT the reader channel.
//!   This avoids the critical bug where poll_exit() consumed and dropped data messages.
//! - The reader thread sends Data/Eof messages; read() drains ALL available messages.
//! - flush() errors are propagated, not silently ignored.

#[cfg(windows)]
mod inner {
    use std::io::{Read, Write};
    use std::sync::mpsc::{self, Receiver, TryRecvError};

    use portable_pty::{native_pty_system, PtySize, CommandBuilder};
    use tracing::{debug, info, warn, error};

    use crate::session::launch::TerminalSize;
    use crate::session::pty::{PtyError, PtyProcess, ReadResult};
    use crate::session::types::ProcessId;

    /// Background reader message.
    enum ReaderMsg {
        Data(Vec<u8>),
        Eof,
    }

    /// Windows ConPTY process using portable-pty.
    pub struct ConPtyProcess {
        pair: Option<portable_pty::PtyPair>,
        writer: Option<Box<dyn Write + Send>>,
        reader_rx: Option<Receiver<ReaderMsg>>,
        /// Child process handle — used for exit detection via try_wait().
        child: Option<Box<dyn portable_pty::Child + Send>>,
        pid: Option<u32>,
        exited: bool,
        exit_code: Option<i32>,
    }

    impl ConPtyProcess {
        pub fn new() -> Self {
            ConPtyProcess {
                pair: None,
                writer: None,
                reader_rx: None,
                child: None,
                pid: None,
                exited: false,
                exit_code: None,
            }
        }
    }

    impl Drop for ConPtyProcess {
        fn drop(&mut self) {
            // Drop writer first to signal EOF
            self.writer = None;
            // Drop pair to clean up PTY
            self.pair = None;
            // Channel is cleaned up automatically
        }
    }

    unsafe impl Send for ConPtyProcess {}
    unsafe impl Sync for ConPtyProcess {}

    impl PtyProcess for ConPtyProcess {
        fn start(
            &mut self,
            spec: &crate::session::launch::LaunchSpec,
        ) -> Result<ProcessId, PtyError> {
            if self.pair.is_some() || self.pid.is_some() {
                return Err(PtyError::AlreadyStarted);
            }

            let pty_system = native_pty_system();
            let pair = pty_system
                .openpty(PtySize {
                    rows: spec.terminal_size.rows,
                    cols: spec.terminal_size.columns,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| PtyError::ForkFailed(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("openpty failed: {}", e),
                )))?;

            // Build command
            let mut cmd = CommandBuilder::new(&spec.executable);
            for arg in &spec.arguments {
                cmd.arg(arg);
            }
            cmd.cwd(&spec.working_directory);

            // Ensure HOME matches USERPROFILE on Windows.
            // When the server is launched from Git Bash, HOME is set to
            // /c/Users/Steve (Unix path). ConPTY inherits this, and child
            // tools like Claude CLI then fail to find ~/.claude/settings.json
            // because /c/Users/Steve is not a valid Windows path.
            // Fix: force HOME = USERPROFILE so it's always a Windows path.
            if let Ok(userprofile) = std::env::var("USERPROFILE") {
                cmd.env("HOME", &userprofile);
                debug!("Set HOME={} (from USERPROFILE) for child process", userprofile);
            }

            // Apply any environment overrides from the session launch spec.
            for (key, value) in &spec.effective_environment.overrides {
                debug!("Env override: {}=<{} chars>", key, value.len());
                cmd.env(key, value);
            }

            // Spawn the child process
            let child = pair.slave
                .spawn_command(cmd)
                .map_err(|e| PtyError::ExecFailed(format!("spawn_command failed: {}", e)))?;

            let pid = child.process_id().expect("child process should have PID");

            // Get writer for input
            let mut writer = pair.master.take_writer()
                .map_err(|e| PtyError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("take_writer failed: {}", e)
                )))?;

            // Get reader and spawn background thread
            let reader = pair.master.try_clone_reader()
                .map_err(|e| PtyError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("try_clone_reader failed: {}", e)
                )))?;

            let (tx, rx) = mpsc::channel::<ReaderMsg>();

            std::thread::spawn(move || {
                let mut reader = reader;
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => {
                            let _ = tx.send(ReaderMsg::Eof);
                            break;
                        }
                        Ok(n) => {
                            if tx.send(ReaderMsg::Data(buf[..n].to_vec())).is_err() {
                                break; // Receiver dropped
                            }
                        }
                        Err(e) => {
                            debug!("PTY reader thread error: {}", e);
                            let _ = tx.send(ReaderMsg::Eof);
                            break;
                        }
                    }
                }
            });

            // Respond to ConPTY DSR query (\x1b[6n) to allow child to initialize.
            // ConPTY sends this during startup and expects a cursor position response.
            // We respond with \x1b[1;1R (cursor at row 1, col 1).
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = writer.write_all(b"\x1b[1;1R");
            let _ = writer.flush();

            info!("PTY started: pid={}, size={}x{}", pid, spec.terminal_size.columns, spec.terminal_size.rows);

            self.pair = Some(pair);
            self.writer = Some(writer);
            self.reader_rx = Some(rx);
            self.child = Some(child);
            self.pid = Some(pid);

            Ok(pid as ProcessId)
        }

        fn write(&mut self, input: &[u8]) -> Result<(), PtyError> {
            let writer = self.writer.as_mut().ok_or_else(|| {
                PtyError::Io(std::io::Error::new(std::io::ErrorKind::NotConnected, "PTY not started"))
            })?;

            if input.is_empty() {
                return Ok(());
            }

            // Log input bytes for debugging
            let hex_preview: String = input.iter()
                .take(32)
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" ");
            let text_preview = String::from_utf8_lossy(&input[..input.len().min(64)]);
            debug!("PTY write: {} bytes [{}] {:?}", input.len(), hex_preview, text_preview);

            writer.write_all(input).map_err(|e| {
                PtyError::Io(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    format!("PTY write failed: {}", e),
                ))
            })?;

            // Flush immediately — critical for interactive terminal responsiveness.
            // Every keystroke must reach the child process without delay.
            if let Err(e) = writer.flush() {
                error!("PTY flush failed: {}", e);
                return Err(PtyError::Io(e));
            }

            Ok(())
        }

        fn read(&mut self, _timeout_ms: u32) -> ReadResult {
            let rx = match self.reader_rx.as_ref() {
                Some(r) => r,
                None => return ReadResult { data: vec![], closed: true },
            };

            if self.exited {
                return ReadResult { data: vec![], closed: true };
            }

            // Drain ALL available messages from the reader thread.
            // This is critical: the reader thread may have queued multiple
            // Data messages between our read() calls. We must collect them all.
            let mut combined = Vec::new();
            let mut saw_eof = false;

            loop {
                match rx.try_recv() {
                    Ok(ReaderMsg::Data(data)) => {
                        combined.extend_from_slice(&data);
                    }
                    Ok(ReaderMsg::Eof) => {
                        saw_eof = true;
                        // Continue draining — there may be data queued before the Eof
                        continue;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        saw_eof = true;
                        break;
                    }
                }
            }

            if saw_eof {
                self.exited = true;
                self.exit_code = Some(0);
            }

            if !combined.is_empty() {
                let text_preview = String::from_utf8_lossy(&combined[..combined.len().min(64)]);
                debug!("PTY read: {} bytes {:?}", combined.len(), text_preview);
            }

            ReadResult {
                data: combined,
                closed: saw_eof,
            }
        }

        fn resize(&mut self, size: TerminalSize) -> Result<(), PtyError> {
            let pair = self.pair.as_ref().ok_or_else(|| {
                PtyError::Io(std::io::Error::new(std::io::ErrorKind::NotConnected, "PTY not started"))
            })?;

            pair.master.resize(PtySize {
                rows: size.rows,
                cols: size.columns,
                pixel_width: 0,
                pixel_height: 0,
            }).map_err(|e| PtyError::Io(
                std::io::Error::new(std::io::ErrorKind::Other, format!("resize failed: {}", e))
            ))?;

            debug!("PTY resized to {}x{}", size.columns, size.rows);
            Ok(())
        }

        fn poll_exit(&mut self) -> Option<i32> {
            if self.exited {
                return self.exit_code;
            }

            // Use the child process handle for exit detection.
            // This is the CORRECT approach — it does NOT consume reader channel data.
            if let Some(child) = self.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let code = status.exit_code() as i32;
                        self.exited = true;
                        self.exit_code = Some(code);
                        info!("Child process exited with code {}", code);
                        Some(code)
                    }
                    Ok(None) => None, // Still running
                    Err(e) => {
                        warn!("try_wait error: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        }

        fn terminate(&mut self) -> bool {
            if self.exited {
                return true;
            }

            self.writer = None; // Close writer to signal EOF
            self.reader_rx = None; // Drop receiver
            self.pair = None; // Drop PTY pair
            self.child = None; // Drop child handle

            self.exited = true;
            self.exit_code = Some(-1);

            if let Some(pid) = self.pid {
                info!("PTY terminated: pid={}", pid);
            }
            true
        }
    }
}

#[cfg(windows)]
pub use inner::ConPtyProcess;

// ──── Tests ────

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use super::*;
    use crate::session::env::EffectiveEnvironment;
    use crate::session::launch::LaunchSpec;
    use crate::session::pty::PtyProcess;
    use crate::session::types::ProviderType;

    fn test_launch_spec() -> LaunchSpec {
        LaunchSpec {
            provider: ProviderType::Codex,
            executable: "cmd.exe".to_string(),
            arguments: vec![],
            effective_environment: EffectiveEnvironment::default(),
            working_directory: "C:\\".to_string(),
            terminal_size: crate::session::launch::TerminalSize {
                columns: 80,
                rows: 24,
            },
        }
    }

    #[test]
    fn test_conpty_create_and_start() {
        let mut pty = ConPtyProcess::new();
        let spec = test_launch_spec();
        let result = pty.start(&spec);
        assert!(result.is_ok(), "Failed to start PTY: {:?}", result.err());
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(pty.terminate());
    }

    #[test]
    fn test_conpty_double_start_fails() {
        let mut pty = ConPtyProcess::new();
        let spec = test_launch_spec();
        let _ = pty.start(&spec);
        let second = pty.start(&spec);
        assert!(second.is_err());
        pty.terminate();
    }

    #[test]
    fn test_conpty_write_and_read() {
        let mut pty = ConPtyProcess::new();
        let spec = test_launch_spec();
        let result = pty.start(&spec);
        assert!(result.is_ok());

        // Wait for cmd.exe to start and produce banner
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Write a command
        let write_result = pty.write(b"echo hello_portable_pty\r\n");
        assert!(write_result.is_ok(), "Write failed: {:?}", write_result.err());

        // Wait for echo output
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Read output - should contain the banner and echo output
        let mut all_output = Vec::new();
        for _ in 0..20 {
            let read_result = pty.read(100);
            if !read_result.data.is_empty() {
                all_output.extend_from_slice(&read_result.data);
            }
            let text = String::from_utf8_lossy(&all_output);
            if text.contains("hello_portable_pty") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        let text = String::from_utf8_lossy(&all_output);
        assert!(text.contains("hello_portable_pty"),
            "Expected 'hello_portable_pty' in output, got: {:?}", text);
        pty.terminate();
    }

    #[test]
    fn test_conpty_resize() {
        let mut pty = ConPtyProcess::new();
        let spec = test_launch_spec();
        let result = pty.start(&spec);
        assert!(result.is_ok());
        let resize_result = pty.resize(crate::session::launch::TerminalSize {
            columns: 120,
            rows: 40,
        });
        assert!(resize_result.is_ok(), "Resize failed: {:?}", resize_result.err());
        pty.terminate();
    }

    #[test]
    fn test_conpty_poll_exit_while_running() {
        let mut pty = ConPtyProcess::new();
        let spec = test_launch_spec();
        let result = pty.start(&spec);
        assert!(result.is_ok());
        // Process should still be running
        let exit = pty.poll_exit();
        assert!(exit.is_none(), "Process should not have exited yet");
        pty.terminate();
    }

    #[test]
    fn test_conpty_drop_cleans_up() {
        {
            let mut pty = ConPtyProcess::new();
            let spec = test_launch_spec();
            let _ = pty.start(&spec);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    #[test]
    fn test_conpty_survives_delay() {
        let mut pty = ConPtyProcess::new();
        let spec = test_launch_spec();
        let result = pty.start(&spec);
        assert!(result.is_ok(), "Start failed: {:?}", result.err());

        // Delay like HTTP create → WebSocket connect gap
        std::thread::sleep(std::time::Duration::from_millis(1000));

        let write_result = pty.write(b"echo alive_test\r\n");
        assert!(write_result.is_ok(), "Write after delay failed: {:?}", write_result.err());

        std::thread::sleep(std::time::Duration::from_millis(500));

        let mut got_output = false;
        for _ in 0..20 {
            let read_result = pty.read(100);
            if !read_result.data.is_empty() {
                let text = String::from_utf8_lossy(&read_result.data);
                if text.contains("alive_test") {
                    got_output = true;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(got_output, "Should have received echo output after delay");
        pty.terminate();
    }
}