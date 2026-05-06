//! POSIX PTY process implementation (Unix-only)
//!
//! Uses forkpty/exec to spawn a child process with a pseudo-terminal.
//! Supports:
//! - Error pipe for early failure detection
//! - Shell mode (execvp, inherits daemon env) vs Clean/Bootstrap (execve, explicit envp)
//! - SIGTERM → SIGKILL graceful termination
//! - Trace logging via VIBE_PTY_TRACE_PATH env var

#[cfg(unix)]
mod inner {
    use std::ffi::CString;
    use std::io::{self, Read as IoRead};
    use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
    use std::path::Path;
    use std::time::{Duration, Instant};

    use nix::errno::Errno;
    use nix::fcntl::{fcntl, FcntlArg, FdFlag};
    use nix::pty::ForkptyResult;
    use nix::sys::signal::{self, Signal};
    use nix::sys::termios::Winsize;
    use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
    use nix::unistd::{chdir, close, execve, execvp, pipe, read, write, Pid};

    use crate::session::env::EnvMode;
    use crate::session::launch::TerminalSize;
    use crate::session::pty::{PtyError, PtyProcess, ReadResult};
    use crate::session::types::ProcessId;

    const TERMINATE_GRACE_PERIOD_MS: u64 = 1500;
    const TERMINATE_POLL_INTERVAL_MS: u64 = 25;
    const READ_BUFFER_SIZE: usize = 4096;

    /// POSIX PTY process using forkpty.
    pub struct PosixPtyProcess {
        master_fd: Option<RawFd>,
        pid: Option<Pid>,
    }

    impl PosixPtyProcess {
        pub fn new() -> Self {
            PosixPtyProcess {
                master_fd: None,
                pid: None,
            }
        }

        fn close_master_fd(&mut self) {
            if let Some(fd) = self.master_fd.take() {
                let _ = close(fd);
            }
        }

        fn make_winsize(size: TerminalSize) -> Winsize {
            Winsize {
                ws_row: size.rows,
                ws_col: size.columns,
                ws_xpixel: 0,
                ws_ypixel: 0,
            }
        }
    }

    impl Drop for PosixPtyProcess {
        fn drop(&mut self) {
            let _ = self.terminate();
            self.close_master_fd();
        }
    }

    impl PtyProcess for PosixPtyProcess {
        fn start(
            &mut self,
            spec: &crate::session::launch::LaunchSpec,
        ) -> Result<ProcessId, PtyError> {
            if self.master_fd.is_some() || self.pid.is_some() {
                return Err(PtyError::AlreadyStarted);
            }

            // Create error pipe with CLOEXEC
            let (read_fd, write_fd) = pipe().map_err(PtyError::Io)?;
            set_cloexec(read_fd)?;
            set_cloexec(write_fd)?;

            let winsize = Self::make_winsize(spec.terminal_size);

            let result = nix::pty::forkpty(Some(&winsize), None).map_err(|e| {
                let _ = close(read_fd);
                let _ = close(write_fd);
                PtyError::ForkFailed(std::io::Error::from_raw_os_error(e as i32))
            })?;

            match result {
                ForkptyResult::Child => {
                    // Child process
                    let _ = close(read_fd);

                    // Change working directory
                    if let Err(e) = chdir(Path::new(&spec.working_directory)) {
                        let errno_val = e as i32;
                        let bytes = unsafe {
                            libc::write(write_fd.as_raw_fd(), &errno_val as *const i32 as *const libc::c_void, std::mem::size_of::<i32>())
                        };
                        std::mem::forget(bytes);
                        unsafe { libc::_exit(127); }
                    }

                    // Build argv
                    let argv = build_argv(&spec.executable, &spec.arguments);

                    if spec.effective_environment.mode == EnvMode::Shell {
                        // Shell mode: use execvp (inherits daemon environment)
                        let c_exec = CString::new(spec.executable.clone()).unwrap_or_default();
                        if let Err(_) = execvp(&c_exec, &argv) {
                            report_exec_error(write_fd.as_raw_fd());
                        }
                    } else {
                        // Clean/Bootstrap: use execve with explicit envp
                        let env_strings = build_env_strings(&spec.effective_environment.entries);
                        let envp = string_vec_to_c_ptrs(&env_strings);
                        let resolved = resolve_executable_path(
                            &spec.executable,
                            &spec.effective_environment.entries,
                        );
                        let c_exec = CString::new(resolved).unwrap_or_default();
                        if let Err(_) = execve(&c_exec, &argv, &envp) {
                            report_exec_error(write_fd.as_raw_fd());
                        }
                    }

                    // exec failed (should not reach here)
                    unsafe { libc::_exit(127); }
                }
                ForkptyResult::Parent { master, child } => {
                    // Parent process
                    let _ = close(write_fd);

                    // Read error pipe
                    let mut child_error: i32 = 0;
                    let mut error_bytes = [0u8; std::mem::size_of::<i32>()];
                    let mut file = unsafe { std::fs::File::from_raw_fd(read_fd.as_raw_fd()) };
                    let bytes_read = file.read(&mut error_bytes).unwrap_or(0);
                    let _ = file.into_raw_fd(); // prevent drop closing it
                    let _ = close(read_fd);

                    if bytes_read >= std::mem::size_of::<i32>() {
                        child_error = i32::from_ne_bytes(error_bytes);
                        let _ = waitpid(child, None);
                        self.close_master_fd();
                        let msg = std::io::Error::from_raw_os_error(child_error)
                            .to_string();
                        return Err(PtyError::ExecFailed(msg));
                    }

                    self.master_fd = Some(master.as_raw_fd());
                    // Leak the OwnedFd so we control when it's closed
                    let _ = master.into_raw_fd();
                    self.pid = Some(child);
                    Ok(child.as_raw() as ProcessId)
                }
            }
        }

        fn write(&mut self, input: &[u8]) -> Result<(), PtyError> {
            let fd = self.master_fd.ok_or_else(|| {
                PtyError::Io(io::Error::new(io::ErrorKind::NotConnected, "PTY not started"))
            })?;

            let mut written = 0;
            while written < input.len() {
                let n = nix::unistd::write(fd, &input[written..])
                    .map_err(|e| PtyError::Io(io::Error::from_raw_os_error(e as i32)))?;
                written += n;
            }
            Ok(())
        }

        fn read(&mut self, timeout_ms: u32) -> ReadResult {
            let fd = match self.master_fd {
                Some(fd) => fd,
                None => return ReadResult { data: vec![], closed: true },
            };

            // Use select to wait for data with timeout
            let mut read_fds = unsafe { std::mem::zeroed() };
            unsafe { libc::FD_ZERO(&mut read_fds) };
            unsafe { libc::FD_SET(fd, &mut read_fds) };

            let mut tv = libc::timeval {
                tv_sec: (timeout_ms / 1000) as libc::time_t,
                tv_usec: ((timeout_ms % 1000) * 1000) as libc::suseconds_t,
            };

            let select_result = unsafe {
                libc::select(fd + 1, &mut read_fds, std::ptr::null_mut(), std::ptr::null_mut(), &mut tv)
            };

            if select_result <= 0 {
                return ReadResult { data: vec![], closed: false };
            }

            let mut buffer = [0u8; READ_BUFFER_SIZE];
            let bytes_read = match nix::unistd::read(fd, &mut buffer) {
                Ok(0) => return ReadResult { data: vec![], closed: true },
                Ok(n) => n,
                Err(Errno::EIO) => return ReadResult { data: vec![], closed: true },
                Err(_) => return ReadResult { data: vec![], closed: false },
            };

            ReadResult {
                data: buffer[..bytes_read].to_vec(),
                closed: false,
            }
        }

        #[cfg(unix)]
        fn readable_fd(&self) -> Option<RawFd> {
            self.master_fd
        }

        fn resize(&mut self, size: TerminalSize) -> Result<(), PtyError> {
            let fd = self.master_fd.ok_or_else(|| {
                PtyError::Io(io::Error::new(io::ErrorKind::NotConnected, "PTY not started"))
            })?;

            let winsize = Self::make_winsize(size);
            let result = unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &winsize) };
            if result < 0 {
                return Err(PtyError::Io(io::Error::last_os_error()));
            }
            Ok(())
        }

        fn poll_exit(&mut self) -> Option<i32> {
            let child_pid = self.pid?;

            match waitpid(child_pid, Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::Exited(_, code)) => {
                    self.close_master_fd();
                    self.pid = None;
                    Some(code)
                }
                Ok(WaitStatus::Signaled(_, sig, _)) => {
                    self.close_master_fd();
                    self.pid = None;
                    Some(128 + sig as i32)
                }
                Ok(WaitStatus::StillAlive) => None,
                Err(_) => None,
                _ => None,
            }
        }

        fn terminate(&mut self) -> bool {
            let child_pid = match self.pid {
                Some(pid) => pid,
                None => return false,
            };

            // Send SIGTERM
            if let Err(e) = signal::kill(child_pid, Signal::SIGTERM) {
                if e != Errno::ESRCH {
                    return false;
                }
            }

            // Wait for graceful exit
            if wait_for_exit(child_pid, TERMINATE_GRACE_PERIOD_MS).is_none() {
                // Send SIGKILL
                if let Err(e) = signal::kill(child_pid, Signal::SIGKILL) {
                    if e != Errno::ESRCH {
                        return false;
                    }
                }

                // Reap the child
                match waitpid(child_pid, None) {
                    Err(e) if e != Errno::ECHILD => return false,
                    _ => {}
                }
            }

            self.close_master_fd();
            self.pid = None;
            true
        }
    }

    // --- Helper functions ---

    fn set_cloexec(fd: RawFd) -> Result<(), PtyError> {
        let flags = fcntl(fd, FcntlArg::F_GETFD).map_err(|e| {
            PtyError::Io(io::Error::from_raw_os_error(e as i32))
        })?;
        fcntl(fd, FcntlArg::F_SETFD(FdFlag::from_bits_truncate(flags) | FdFlag::FD_CLOEXEC))
            .map_err(|e| PtyError::Io(io::Error::from_raw_os_error(e as i32)))?;
        Ok(())
    }

    fn build_argv(executable: &str, arguments: &[String]) -> Vec<CString> {
        let mut argv = Vec::with_capacity(arguments.len() + 2);
        argv.push(CString::new(executable.to_string()).unwrap_or_default());
        for arg in arguments {
            argv.push(CString::new(arg.clone()).unwrap_or_default());
        }
        argv
    }

    fn build_env_strings(entries: &[crate::session::env::EnvEntry]) -> Vec<String> {
        entries.iter().map(|e| format!("{}={}", e.key, e.value)).collect()
    }

    fn string_vec_to_c_ptrs(strings: &[String]) -> Vec<CString> {
        strings
            .iter()
            .map(|s| CString::new(s.clone()).unwrap_or_default())
            .collect()
    }

    fn resolve_executable_path(
        executable: &str,
        entries: &[crate::session::env::EnvEntry],
    ) -> String {
        // If it contains a slash, use as-is
        if executable.contains('/') {
            return executable.to_string();
        }

        // Find PATH from entries
        let path_value = entries
            .iter()
            .find(|e| e.key == "PATH")
            .map(|e| e.value.as_str())
            .unwrap_or("/usr/bin:/bin:/usr/sbin:/sbin");

        for dir in path_value.split(':') {
            let directory = if dir.is_empty() { "." } else { dir };
            let candidate = format!("{}/{}", directory, executable);
            let c_path = match CString::new(candidate.clone()) {
                Ok(c) => c,
                Err(_) => continue,
            };
            // Check if executable
            if unsafe { libc::access(c_path.as_ptr(), libc::X_OK) } == 0 {
                return candidate;
            }
        }

        executable.to_string()
    }

    fn report_exec_error(write_fd: RawFd) {
        let errno_val = std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as i32;
        unsafe {
            libc::write(
                write_fd,
                &errno_val as *const i32 as *const libc::c_void,
                std::mem::size_of::<i32>(),
            );
        }
    }

    fn wait_for_exit(pid: Pid, timeout_ms: u64) -> Option<i32> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        while Instant::now() < deadline {
            match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::Exited(_, code)) => return Some(code),
                Ok(WaitStatus::Signaled(_, sig, _)) => return Some(128 + sig as i32),
                Err(Errno::ECHILD) => return Some(0),
                Err(_) => return None,
                _ => {}
            }
            std::thread::sleep(Duration::from_millis(TERMINATE_POLL_INTERVAL_MS));
        }
        None
    }
}

#[cfg(unix)]
pub use inner::PosixPtyProcess;
