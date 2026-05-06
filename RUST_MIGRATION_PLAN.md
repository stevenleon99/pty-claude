# Sentrits-Core C++ to Rust Migration Plan

## Executive Summary

Sentrits-Core is a sophisticated terminal session management daemon (~40 source files, ~10k lines) that provides:
- PTY-based terminal session lifecycle management
- HTTP/WebSocket server for remote terminal access
- Device pairing and authentication
- Hub integration for relay/remote control
- Evidence/log capture and observation storage
- Git workspace inspection

The migration will preserve all functionality while leveraging Rust's safety guarantees, async capabilities, and modern ecosystem.

---

## 1. Project Architecture Overview

### 1.1 Module Structure (C++)

```
include/vibe/
├── auth/           # Authentication & device pairing
│   ├── authorizer.h          (interface: Authorizer)
│   ├── default_authorizer.h  (implementation)
│   ├── pairing.h             (interface: PairingService, data types)
│   └── default_pairing_service.h
├── base/            # Debug utilities
│   └── debug_trace.h
├── cli/             # Daemon client for CLI
│   └── daemon_client.h
├── net/             # Networking
│   ├── discovery.h           (UDP broadcast discovery)
│   ├── discovery_broadcaster.h
│   ├── http_server.h         (main HTTP/WebSocket server)
│   ├── http_shared.h         (shared HTTP types)
│   ├── websocket_shared.h    (WebSocket utilities)
│   ├── hub_client.h          (Hub heartbeat integration)
│   ├── hub_control_channel.h (Hub relay control)
│   ├── local_auth.h          (Local auth service factory)
│   ├── request_parsing.h     (WebSocket command parsing)
│   └── json.h                (JSON serialization)
├── service/         # Business logic services
│   ├── session_manager.h     (core session orchestration)
│   ├── log_buffer.h          (circular log buffer)
│   ├── managed_log_process.h (managed process execution)
│   ├── observation_store.h   (event observation storage)
│   ├── evidence.h            (evidence data types)
│   ├── evidence_response_assembler.h
│   ├── git_inspector.h       (git state inspection)
│   └── workspace_file_watcher.h (inotify-based file watching)
├── session/         # Session management
│   ├── pty_process.h         (interface: IPtyProcess)
│   ├── posix_pty_process.h   (Linux/macOS PTY)
│   ├── pty_process_factory.h (platform detection)
│   ├── session_runtime.h     (session execution)
│   ├── session_record.h      (session state)
│   ├── session_lifecycle.h   (state machine)
│   ├── session_output_buffer.h
│   ├── session_snapshot.h    (snapshot data types)
│   ├── session_types.h       (core types: SessionId, etc.)
│   ├── terminal_multiplexer.h (vterm-based terminal)
│   ├── terminal_debug_artifact.h
│   ├── env_config.h          (environment configuration)
│   ├── launch_spec.h         (process launch specification)
│   ├── provider_config.h
│   └── bootstrapped_env_cache.h
└── store/           # Persistence
    ├── file_stores.h         (file-based implementations)
    ├── host_config_store.h   (host configuration)
    ├── pairing_store.h       (pairing records)
    └── session_store.h       (session persistence)
```

### 1.2 Key Dependencies (C++)

| C++ Dependency | Rust Equivalent |
|----------------|-----------------|
| Boost.JSON | `serde_json` or `simd-json` |
| Boost.Beast (HTTP/WS) | `axum` + `tokio-tungstenite` or `warp` |
| OpenSSL | `rustls` or `openssl` crate |
| libvterm | `vte` crate or bind to libvterm via `cc` |
| POSIX APIs (forkpty, etc.) | `nix` crate + `portable-pty` or custom |
| inotify (Linux) | `notify` crate |
| GoogleTest | `cargo test` + `rstest` or `proptest` |

### 1.3 Threading Model

- **HTTP Server**: Boost.Asio with `io_context` (single-threaded event loop)
- **Discovery Broadcaster**: Dedicated thread with condition variable
- **Hub Client**: Background thread for heartbeats
- **Hub Control Channel**: Control thread + bridge threads
- **PTY Reading**: Blocking reads with select/poll
- **File Watcher**: inotify-based polling (Linux) or scan-based (macOS)

**Rust Migration**: Use `tokio` async runtime with:
- Single-threaded HTTP server (axum)
- `tokio::task::spawn_blocking` for PTY I/O
- `tokio::sync::broadcast` for discovery
- `tokio::time::interval` for periodic tasks

---

## 2. Rust Project Structure

```
sentrits-core/
├── Cargo.toml
├── Cargo.lock
├── .cargo/
│   └── config.toml          # Platform-specific config
├── src/
│   ├── main.rs              # Entry point, CLI parsing
│   ├── lib.rs               # Library root
│   │
│   ├── auth/
│   │   ├── mod.rs
│   │   ├── authorizer.rs    # Auth trait + RequestContext, AuthResult
│   │   ├── pairing.rs       # PairingService trait, data types
│   │   └── default.rs       # Default implementations
│   │
│   ├── cli/
│   │   ├── mod.rs
│   │   └── daemon_client.rs # HTTP client for daemon
│   │
│   ├── net/
│   │   ├── mod.rs
│   │   ├── http_server.rs   # Axum HTTP/WebSocket server
│   │   ├── discovery.rs     # UDP broadcast
│   │   ├── hub_client.rs    # Hub integration
│   │   ├── hub_control.rs   # Relay control channel
│   │   └── json.rs          # JSON types (serde)
│   │
│   ├── service/
│   │   ├── mod.rs
│   │   ├── session_manager.rs
│   │   ├── log_buffer.rs
│   │   ├── managed_process.rs
│   │   ├── observation.rs
│   │   ├── evidence.rs
│   │   ├── git_inspector.rs
│   │   └── file_watcher.rs
│   │
│   ├── session/
│   │   ├── mod.rs
│   │   ├── pty.rs           # PTY trait + platform implementations
│   │   ├── pty_unix.rs      # Unix PTY (#[cfg(unix)])
│   │   ├── runtime.rs       # SessionRuntime
│   │   ├── record.rs        # SessionRecord
│   │   ├── lifecycle.rs     # State machine
│   │   ├── output_buffer.rs
│   │   ├── snapshot.rs
│   │   ├── types.rs         # SessionId, SessionStatus, etc.
│   │   ├── terminal.rs      # Terminal multiplexer
│   │   ├── env.rs           # Environment configuration
│   │   └── launch.rs        # LaunchSpec
│   │
│   ├── store/
│   │   ├── mod.rs
│   │   ├── file_store.rs    # File-based persistence
│   │   ├── host_config.rs
│   │   ├── pairing_store.rs
│   │   └── session_store.rs
│   │
│   └── util/
│       ├── mod.rs
│       └── debug.rs
│
├── tests/                    # Integration tests
│   ├── auth_test.rs
│   ├── session_test.rs
│   └── ...
│
└── benches/                  # Performance benchmarks (optional)
    └── ...
```

### 2.1 Cargo.toml (Draft)

```toml
[package]
name = "sentrits-core"
version = "0.2.5"
edition = "2021"
rust-version = "1.75"
license = "MIT"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# HTTP/WebSocket
axum = { version = "0.7", features = ["ws", "macros"] }
axum-extra = { version = "0.9", features = ["typed-header"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["fs", "cors", "trace"] }
tokio-tungstenite = "0.21"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# TLS
rustls = "0.23"
tokio-rustls = "0.26"
rustls-pemfile = "2"

# Terminal/PTY
nix = { version = "0.28", features = ["term", "process", "signal", "fs"] }
vte = "0.13"                    # Terminal parsing (alternative to libvterm)

# File watching
notify = "6"

# Git inspection
git2 = "0.18"                   # libgit2 bindings

# CLI
clap = { version = "4", features = ["derive", "env"] }

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# Utilities
thiserror = "1"
anyhow = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
bytes = "1"
futures = "0.3"
pin-project-lite = "0.2"

# Platform-specific
[target.'cfg(unix)'.dependencies]
libc = "0.2"

[target.'cfg(target_os = "linux")'.dependencies]
inotify = "0.10"

[dev-dependencies]
tokio-test = "0.4"
rstest = "0.18"
proptest = "1"
tempfile = "3"
assert_matches = "1"

[[bin]]
name = "sentrits"
path = "src/main.rs"

[lib]
name = "sentrits_core"
path = "src/lib.rs"
```

---

## 3. C++ to Rust Translation Guide

### 3.1 Ownership Patterns

| C++ Pattern | Rust Translation |
|-------------|------------------|
| `std::unique_ptr<T>` | `Box<T>` or direct ownership |
| `std::shared_ptr<T>` | `Arc<T>` (with `Mutex`/`RwLock` if mutable) |
| `T&` (non-owning) | `&T` or `&mut T` |
| `T*` (raw, owning) | `Box<T>` or `unsafe` with manual drop |
| `T*` (raw, non-owning) | `*const T` / `*mut T` with lifetime comments, or ref if possible |

### 3.2 Interface Translation

**C++ Abstract Class:**
```cpp
class IPtyProcess {
 public:
  virtual ~IPtyProcess() = default;
  virtual auto Start(const LaunchSpec& spec) -> StartResult = 0;
  virtual auto Write(std::string_view input) -> bool = 0;
  virtual auto Read(int timeout_ms) -> ReadResult = 0;
};
```

**Rust Trait:**
```rust
pub trait PtyProcess: Send + Sync {
    fn start(&mut self, spec: &LaunchSpec) -> Result<ProcessId, PtyError>;
    fn write(&mut self, input: &[u8]) -> Result<(), PtyError>;
    fn read(&mut self, timeout_ms: u32) -> Result<Vec<u8>, PtyError>;
    fn resize(&mut self, size: TerminalSize) -> Result<(), PtyError>;
    fn poll_exit(&mut self) -> Option<i32>;
    fn terminate(&mut self) -> bool;
}
```

### 3.3 Error Handling

**C++ (error codes + optional):**
```cpp
struct StartResult {
  bool started{false};
  ProcessId pid{0};
  std::string error_message;
};
```

**Rust (Result + thiserror):**
```rust
#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    #[error("process already started")]
    AlreadyStarted,
    #[error("fork failed: {0}")]
    ForkFailed(#[source] nix::Error),
    #[error("exec failed: {0}")]
    ExecFailed(String),
}

pub type PtyResult<T> = Result<T, PtyError>;
```

### 3.4 Concurrency Patterns

**C++ (mutex + condition variable):**
```cpp
std::mutex mutex_;
std::condition_variable cv_;
bool stopping_{false};

// Wait with timeout
std::unique_lock lock(mutex_);
cv_.wait_for(lock, interval, [&] { return stopping_; });
```

**Rust (tokio sync):**
```rust
let mut interval = tokio::time::interval(Duration::from_secs(1));
loop {
    tokio::select! {
        _ = interval.tick() => { /* broadcast */ }
        _ = stop_rx.changed() => { break; }
    }
}
```

### 3.5 Platform-Specific Code

**C++:**
```cpp
#if defined(__APPLE__)
#include <util.h>
#elif defined(__linux__)
#include <pty.h>
#else
#error "Only supported on macOS and Linux"
#endif
```

**Rust:**
```rust
// src/session/pty_unix.rs
#[cfg(unix)]
mod unix_pty {
    use nix::unistd::*;
    // ... Unix-specific PTY implementation
}

// src/session/pty.rs
#[cfg(unix)]
pub use self::pty_unix::UnixPty as PlatformPty;
```

---

## 4. Migration Phases

### Phase 1: Foundation (Week 1-2)
**Goal**: Set up project structure, core types, and basic infrastructure.

1. **Project Setup**
   - Create Cargo.toml with dependencies
   - Set up workspace structure
   - Configure CI (GitHub Actions)
   - Add basic logging/tracing

2. **Core Types** (`session/types.rs`, `auth/mod.rs`)
   - `SessionId` (newtype wrapper)
   - `SessionStatus` (enum)
   - `ProviderType` (enum)
   - `DeviceId`, `DeviceType`
   - `TerminalSize`
   - All JSON-serializable types with `serde`

3. **Error Types**
   - Define error enums with `thiserror`
   - Create `Result<T>` type aliases per module

4. **Tests**: Unit tests for all core types

### Phase 2: Session Layer (Week 2-4)
**Goal**: Implement PTY process management and session runtime.

1. **PTY Interface** (`session/pty.rs`)
   - Define `PtyProcess` trait
   - Implement `UnixPty` using `nix` crate
   - Handle forkpty, exec, signal handling
   - **Risk**: Complex Unix APIs, needs careful testing

2. **Environment Configuration** (`session/env.rs`)
   - `EnvMode` enum
   - `EnvConfig`, `EffectiveEnvironment`
   - Environment file parsing

3. **Launch Specification** (`session/launch.rs`)
   - `LaunchSpec` struct
   - Process spawning logic

4. **Session Output Buffer** (`session/output_buffer.rs`)
   - Ring buffer implementation
   - Sequence tracking

5. **Terminal Multiplexer** (`session/terminal.rs`)
   - Integrate `vte` crate for terminal parsing
   - Screen state management
   - Viewport handling

6. **Session Runtime** (`session/runtime.rs`)
   - `SessionRecord` state
   - `SessionRuntime` orchestrating PTY + buffer + terminal

7. **Tests**: PTY integration tests, buffer tests, terminal tests

### Phase 3: Store Layer (Week 4-5)
**Goal**: Implement persistence layer.

1. **Session Store** (`store/session_store.rs`)
   - Trait definition
   - File-based implementation
   - JSON serialization

2. **Host Config Store** (`store/host_config.rs`)
   - Host identity persistence
   - Provider command overrides

3. **Pairing Store** (`store/pairing_store.rs`)
   - Pairing record persistence

4. **Tests**: File store tests with temp directories

### Phase 4: Service Layer (Week 5-7)
**Goal**: Implement business logic services.

1. **Log Buffer** (`service/log_buffer.rs`)
   - Circular buffer with limits
   - Search, tail, range operations

2. **Managed Log Process** (`service/managed_process.rs`)
   - Process spawning with output capture
   - Reader threads (tokio tasks)

3. **Git Inspector** (`service/git_inspector.rs`)
   - Use `git2` crate
   - Branch, status, modified files

4. **File Watcher** (`service/file_watcher.rs`)
   - Use `notify` crate
   - Cross-platform file watching

5. **Observation Store** (`service/observation.rs`)
   - Event storage with limits

6. **Session Manager** (`service/session_manager.rs`)
   - Central orchestrator
   - Session lifecycle management
   - Evidence operations

7. **Tests**: Service-level tests with mocks

### Phase 5: Auth Layer (Week 7-8)
**Goal**: Implement authentication and pairing.

1. **Authorizer** (`auth/authorizer.rs`)
   - Trait definition
   - `RequestContext`, `AuthResult`
   - Default implementation

2. **Pairing Service** (`auth/pairing.rs`)
   - Trait definition
   - Pairing flow types
   - Default implementation

3. **Tests**: Auth flow tests

### Phase 6: Network Layer (Week 8-11)
**Goal**: Implement HTTP/WebSocket server.

1. **JSON Types** (`net/json.rs`)
   - All API request/response types
   - Serde serialization

2. **Discovery** (`net/discovery.rs`)
   - UDP broadcast
   - Service announcement

3. **HTTP Server** (`net/http_server.rs`)
   - Axum-based server
   - Admin and remote endpoints
   - TLS support (rustls)
   - Static file serving (web assets)

4. **WebSocket Handling**
   - Terminal output streaming
   - Input handling
   - Session attach/observe

5. **Hub Client** (`net/hub_client.rs`)
   - Heartbeat integration
   - HTTP client (reqwest)

6. **Hub Control Channel** (`net/hub_control.rs`)
   - Relay token management
   - WebSocket bridging

7. **Tests**: HTTP integration tests, WebSocket tests

### Phase 7: CLI Layer (Week 11-12)
**Goal**: Implement command-line interface.

1. **CLI Definition** (`main.rs`)
   - Use `clap` for argument parsing
   - All subcommands from C++ version

2. **Daemon Client** (`cli/daemon_client.rs`)
   - HTTP client for daemon communication
   - Session attach/observe modes

3. **Service Installation**
   - systemd unit generation (Linux)
   - launchd plist generation (macOS)

### Phase 8: Integration & Polish (Week 12-14)
**Goal**: Complete integration, testing, and optimization.

1. **Integration Tests**
   - End-to-end session lifecycle
   - HTTP API tests
   - WebSocket tests

2. **Performance Testing**
   - Benchmark critical paths
   - Memory profiling

3. **Documentation**
   - API documentation (rustdoc)
   - Architecture documentation

4. **Packaging**
   - Debian package (cargo-deb)
   - macOS DMG (cargo-bundle)
   - systemd/launchd integration

---

## 5. Risk Assessment

### 5.1 High Risk Areas

| Area | Risk | Mitigation |
|------|------|------------|
| **PTY Process Management** | Complex Unix APIs, fork safety | Extensive testing, careful unsafe code review |
| **Terminal Multiplexer** | libvterm compatibility | Use `vte` crate or bind to libvterm |
| **WebSocket Streaming** | Backpressure, connection handling | Use established crates (tokio-tungstenite) |
| **Signal Handling** | Race conditions, async safety | Use `tokio::signal`, careful synchronization |
| **File Descriptor Management** | Leaks, race conditions | RAII patterns with `Drop`, careful testing |

### 5.2 Medium Risk Areas

| Area | Risk | Mitigation |
|------|------|------------|
| **Environment Resolution** | Shell integration complexity | Incremental testing, preserve C++ behavior |
| **Git Inspection** | git2 API differences | Test against real repositories |
| **File Watching** | Cross-platform differences | Use `notify` crate, test on both platforms |
| **TLS Configuration** | Certificate handling | Use `rustls`, test with real certs |

### 5.3 Low Risk Areas

| Area | Risk | Mitigation |
|------|------|------------|
| **JSON Serialization** | Type mismatches | Serde tests, JSON schema validation |
| **Store Layer** | File I/O errors | Standard error handling |
| **CLI Parsing** | Argument parsing edge cases | Clap handles most cases |

---

## 6. Unsafe Code Strategy

### 6.1 Where `unsafe` is Necessary

1. **PTY Creation** (`forkpty` syscall)
   - Fork semantics require careful handling
   - File descriptor management
   - Signal handling in child process

2. **Terminal I/O** (`ioctl`, `termios`)
   - Platform-specific terminal control
   - Window size changes

3. **Signal Handling**
   - `sigaction`, `sigwait` equivalents
   - Async-signal-safe code

### 6.2 Safety Guidelines

1. **Minimize unsafe scope**: Wrap in safe abstractions
2. **Document invariants**: Explain safety requirements
3. **Test thoroughly**: Integration tests for all unsafe paths
4. **Use safe alternatives where possible**: `nix` crate provides safe wrappers

### 6.3 Example: Safe PTY Wrapper

```rust
// pty_unix.rs
pub struct UnixPty {
    master_fd: RawFd,
    pid: Pid,
}

impl UnixPty {
    pub fn new(spec: &LaunchSpec) -> Result<Self, PtyError> {
        // Safe wrapper around forkpty
        let (master_fd, pid) = unsafe { self::fork_pty(spec)? };
        Ok(Self { master_fd, pid })
    }
}

impl Drop for UnixPty {
    fn drop(&mut self) {
        // Safe: Just closing file descriptors
        let _ = nix::unistd::close(self.master_fd);
        // Terminate child process if still running
        let _ = self.terminate();
    }
}
```

---

## 7. Testing Strategy

### 7.1 Unit Tests
- Each module has `#[cfg(test)] mod tests`
- Use `rstest` for fixtures
- Use `proptest` for property-based testing

### 7.2 Integration Tests
- `tests/` directory for integration tests
- Use `tempfile` for test isolation
- Test against real PTY processes where possible

### 7.3 Test Coverage
- Aim for >80% coverage
- Focus on critical paths (PTY, session lifecycle, HTTP)

### 7.4 CI Testing
- Run tests on Linux and macOS
- Use GitHub Actions matrix

---

## 8. Compatibility Considerations

### 8.1 API Compatibility
- Preserve HTTP API endpoints
- Preserve WebSocket message formats
- Preserve CLI command structure

### 8.2 Data Compatibility
- Preserve JSON file formats
- Preserve session store format
- Preserve pairing store format

### 8.3 Behavior Compatibility
- Preserve session lifecycle semantics
- Preserve environment resolution behavior
- Preserve terminal handling

---

## 9. Migration Checklist

### Phase 1: Foundation
- [ ] Create Cargo.toml with dependencies
- [ ] Set up project structure
- [ ] Implement core types (SessionId, SessionStatus, etc.)
- [ ] Implement error types
- [ ] Set up CI

### Phase 2: Session Layer
- [ ] Implement PtyProcess trait
- [ ] Implement UnixPty
- [ ] Implement environment configuration
- [ ] Implement LaunchSpec
- [ ] Implement SessionOutputBuffer
- [ ] Implement TerminalMultiplexer
- [ ] Implement SessionRuntime

### Phase 3: Store Layer
- [ ] Implement SessionStore trait
- [ ] Implement FileSessionStore
- [ ] Implement HostConfigStore
- [ ] Implement PairingStore

### Phase 4: Service Layer
- [ ] Implement LogBuffer
- [ ] Implement ManagedLogProcess
- [ ] Implement GitInspector
- [ ] Implement WorkspaceFileWatcher
- [ ] Implement ObservationStore
- [ ] Implement SessionManager

### Phase 5: Auth Layer
- [ ] Implement Authorizer trait
- [ ] Implement DefaultAuthorizer
- [ ] Implement PairingService trait
- [ ] Implement DefaultPairingService

### Phase 6: Network Layer
- [ ] Implement JSON types
- [ ] Implement Discovery
- [ ] Implement HttpServer
- [ ] Implement WebSocket handling
- [ ] Implement HubClient
- [ ] Implement HubControlChannel

### Phase 7: CLI Layer
- [ ] Implement CLI parsing
- [ ] Implement daemon client
- [ ] Implement service installation

### Phase 8: Integration
- [ ] Integration tests
- [ ] Performance testing
- [ ] Documentation
- [ ] Packaging

---

## 10. References

- [Rust nomicon](https://doc.rust-lang.org/nomicon/) - Unsafe Rust patterns
- [tokio tutorial](https://tokio.rs/tokio/tutorial) - Async runtime
- [axum documentation](https://docs.rs/axum) - HTTP framework
- [nix crate](https://docs.rs/nix) - Unix API bindings
- [vte crate](https://docs.rs/vte) - Terminal parsing
