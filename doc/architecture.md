# Architecture

Sentrits-Core is a Rust-based terminal session management daemon that
exposes interactive shell sessions (including Claude CLI) over HTTP and
WebSocket, allowing remote clients to view and control terminal
applications running inside Windows ConPTY pseudo-consoles.

---

## 1. Project Purpose

The system lets a remote device — a phone, tablet, or another computer
— open, observe, and interact with terminal sessions running on a host
machine. The primary use case is running AI coding agents like Claude
CLI inside a pseudo-terminal and streaming the terminal I/O to a remote
client in real time.

---

## 2. High-Level Architecture

```
┌─────────────┐         HTTP / WebSocket          ┌──────────────────┐
│  Remote      │ ◄──────────────────────────────► │  Sentrits Daemon │
│  Client      │   JSON commands + binary I/O      │  (host machine)  │
│  (browser,   │                                   │                  │
│   phone,     │                                   │  ┌────────────┐  │
│   terminal)  │                                   │  │ axum HTTP  │  │
│              │                                   │  │ server     │  │
└─────────────┘                                   │  └─────┬──────┘  │
                                                  │        │         │
                                                  │  ┌─────▼──────┐  │
                                                  │  │ Session    │  │
                                                  │  │ Registry   │  │
                                                  │  └─────┬──────┘  │
                                                  │        │         │
                                                  │  ┌─────▼──────┐  │
                                                  │  │ ConPTY     │  │
                                                  │  │ sessions   │  │
                                                  │  └─────┬──────┘  │
                                                  │        │         │
                                                  │  ┌─────▼──────┐  │
                                                  │  │ Claude CLI │  │
                                                  │  │ / cmd.exe  │  │
                                                  │  └────────────┘  │
                                                  └──────────────────┘
```

The daemon runs on the host machine and listens on two ports:

| Port   | Bind address    | Purpose                     |
|--------|-----------------|-----------------------------|
| Admin  | `127.0.0.1:18085` | Local management API       |
| Remote | `0.0.0.0:18086`   | Remote client connections  |

Both ports serve the same routes; the split exists so a firewall can
block external access to the admin port while allowing remote clients
on the other.

---

## 3. Main Components

### 3.1 ConPTY Session Manager (`session/registry.rs`)

`SessionRegistry` holds every active terminal session in an
`HashMap<String, Arc<RwLock<RunningSession>>>`. It provides:

- `create_session` — builds a `LaunchSpec`, creates a PTY via a factory
  closure, stores the resulting `RunningSession`.
- `get_session` — looks up a session by ID.
- `list_sessions` / `remove_session` — enumeration and cleanup.

Each `RunningSession` wraps a `Box<dyn PtyProcess>` behind an async
`RwLock` so the output pump and input pump can share access.

### 3.2 Child Process Launcher (`session/launch.rs`, `session/conpty.rs`)

`LaunchSpec` describes what to run:

```rust
pub struct LaunchSpec {
    pub provider: ProviderType,        // Codex or Claude
    pub executable: String,            // e.g. "cmd.exe" or "node"
    pub arguments: Vec<String>,
    pub effective_environment: EffectiveEnvironment,
    pub working_directory: String,
    pub terminal_size: TerminalSize,   // columns × rows
}
```

On Windows the `ConPtyProcess` struct implements the `PtyProcess` trait
using the `portable-pty` crate:

1. `native_pty_system().openpty(PtySize{…})` creates the pseudo-console.
2. `CommandBuilder` assembles the child command line.
3. `pair.slave.spawn_command(cmd)` launches the child attached to the
   ConPTY.
4. A background `std::thread` reads from the PTY master in 8 KiB
   chunks and sends `ReaderMsg::Data(bytes)` or `ReaderMsg::Eof` through
   an `mpsc` channel.
5. The main `read()` method does a non-blocking `try_recv()` on that
   channel, never blocking the async runtime.

**DSR handshake.** Windows ConPTY sends `\x1b[6n` (Device Status
Report) during initialization. The child process will not produce
further output until the terminal responds with a cursor-position
report. `ConPtyProcess::start()` handles this automatically by writing
`\x1b[1;1R` (cursor at row 1, column 1) to the master side shortly
after spawn.

### 3.3 Pipe I/O Bridge (`net/websocket.rs`)

The WebSocket bridge at `ws/sessions/{id}` runs two concurrent tokio
tasks:

**Output pump** (PTY → network):

```
loop {
    session.read(0)          // non-blocking channel receive
    → SessionServerMessage::Output { data: String }
    → WebSocket Message::Text(json)
}
```

Polls at ~60 Hz (16 ms sleep when no data).

**Input pump** (network → PTY):

```
loop {
    ws_receiver.next()
    → parse JSON command or raw text/binary
    → session.write(bytes) | session.resize(size) | session.terminate()
}
```

Both pumps break when the process exits or the WebSocket disconnects.

### 3.4 Network Server Layer (`net/`)

```
net/
├── server.rs     TcpListener + axum::serve, CORS, tracing middleware
├── routes.rs     REST handlers (POST /sessions, GET /health, etc.)
├── websocket.rs  WebSocket upgrade handlers and pump loops
├── state.rs      AppState shared between handlers
└── auth.rs       Authentication middleware (stub)
```

`AppState` is cloned freely — every field is wrapped in `Arc` or
`Arc<RwLock<…>>`:

```rust
pub struct AppState {
    pub authorizer:          Arc<dyn Authorizer>,
    pub pairing_service:     Arc<RwLock<dyn PairingService>>,
    pub session_store:       Arc<dyn SessionStore>,
    pub host_config_store:   Arc<dyn HostConfigStore>,
    pub observation_store:   Arc<RwLock<ObservationStore>>,
    pub session_registry:    Arc<SessionRegistry>,
}
```

### 3.5 Remote Terminal Client

The remote client is not part of this crate. Any WebSocket client that
speaks the JSON protocol below can connect. A browser-based xterm.js
terminal is the intended frontend.

**Client → Server messages:**

```jsonc
{ "type": "input",  "data": "ls -la\n" }
{ "type": "resize", "columns": 120, "rows": 40 }
{ "type": "stop" }
```

Raw text or binary WebSocket frames are also accepted and forwarded
directly to the PTY stdin.

**Server → Client messages:**

```jsonc
{ "type": "output",  "data": "file.txt\r\n" }
{ "type": "resized", "columns": 120, "rows": 40 }
{ "type": "exited",  "code": 0 }
{ "type": "state",   "status": "running" }
{ "type": "error",   "message": "session not found" }
```

---

## 4. Data Flow

### 4.1 Upstream (client input to child process)

```
Remote client
  │  WebSocket JSON { type: "input", data: "hello\n" }
  ▼
axum WebSocket handler (input pump)
  │  parse JSON → extract bytes
  ▼
RunningSession::write(bytes)
  │  RwLock write guard
  ▼
ConPtyProcess::write(&input)
  │  portable-pty master.take_writer().write_all()
  ▼
Windows ConPTY kernel driver
  │  routes to child's stdin
  ▼
Claude CLI / cmd.exe reads stdin
```

### 4.2 Downstream (child output to client)

```
Claude CLI / cmd.exe writes to stdout
  │
  ▼
Windows ConPTY kernel driver
  │  routes to master side
  ▼
Background reader thread
  │  reader.read(&mut buf) → mpsc::send(Data(buf))
  ▼
ConPtyProcess::read(0)
  │  mpsc::try_recv() → ReadResult { data, closed }
  ▼
RunningSession::read()
  │  delegates to PtyProcess::read()
  ▼
WebSocket handler (output pump)
  │  SessionServerMessage::Output { data } → JSON → Message::Text
  ▼
Remote client renders output
```

### 4.3 Resize

```
Client → { type: "resize", columns: 120, rows: 40 }
  → RunningSession::resize(TerminalSize)
    → ConPtyProcess::resize()
      → pair.master.resize(PtySize { rows, cols, … })
        → Windows ResizePseudoConsole
          → SIGWINCH equivalent → child redraws
```

---

## 5. Claude CLI Inside ConPTY

Claude CLI (`@anthropic-ai/claude-code`) is a Node.js application that
renders an interactive TUI using the Ink framework (React for CLI).

### Why it needs a PTY, not plain pipes

| Aspect          | Plain pipes              | Pseudo-terminal            |
|-----------------|--------------------------|----------------------------|
| Stdin isatty    | `false`                  | `true`                     |
| ANSI rendering  | Stripped or disabled     | Full color, cursor, TUI    |
| Interactive mode| Programs may batch output| Line-by-line streaming     |
| Line editing    | Raw bytes only           | Readline / prompt support  |
| Resize          | Not possible             | `resize()` → layout update |

Claude CLI checks `isatty(stdout)` and switches between a full TUI
mode (interactive) and a simpler batch mode. Without a PTY the CLI
degrades to non-interactive behavior.

### Launching Claude CLI through the system

Because `claude` is installed as an npm wrapper script (`.cmd` on
Windows, shell script on Unix), the ConPTY must launch `node` directly
with the CLI entry point:

```
node <npm-prefix>/node_modules/@anthropic-ai/claude-code/cli.js
```

Passing the `claude.cmd` wrapper through `cmd.exe` in the ConPTY does
not reliably produce output due to batch-file escaping issues in the
pseudo-terminal environment.

---

## 6. Terminal Behavior

### ANSI escape sequence preservation

All bytes from the ConPTY master are forwarded verbatim. The server does
not parse, filter, or modify ANSI sequences. The remote client is
responsible for rendering them (e.g. via xterm.js).

Common sequences observed from Claude CLI and cmd.exe:

| Sequence        | Meaning                        |
|-----------------|--------------------------------|
| `\x1b[?9001h`   | WinTerm input mode             |
| `\x1b[?1004h`   | Focus reporting                |
| `\x1b[?2026h`   | Synchronized output start      |
| `\x1b[6n`       | Device Status Report query     |
| `\x1b[1;1R`     | Cursor position response       |
| `\x1b[2J`       | Clear screen                   |
| `\x1b[H`        | Cursor home                    |
| `\x1b[?1049h`   | Switch to alternate screen     |

### UTF-8 handling

PTY output bytes are converted to `String` via
`String::from_utf8_lossy()` before JSON serialization. Invalid UTF-8
sequences are replaced with the Unicode replacement character. This is
safe for display but may corrupt binary data.

### Streaming output

The output pump polls at ~60 Hz. Data is sent to the client as soon as
a non-empty `ReadResult` arrives. There is no batching or debouncing —
each channel message triggers one WebSocket send.

### Interactive input

The input pump accepts JSON `{ type: "input", data: "…" }` messages and
raw text/binary WebSocket frames. Keystrokes are forwarded to the PTY
immediately. The server does not interpret line editing — all
processing happens in the child process (e.g. readline).

### Terminal resize

Resize requests from the client are forwarded to
`pair.master.resize()` which calls the Windows `ResizePseudoConsole`
API. The child process receives the new dimensions and should redraw its
TUI layout.

---

## 7. Module Map

```
src/
├── main.rs                          CLI entry point
├── lib.rs                           Library exports
├── cli/mod.rs                       clap command definitions
│
├── auth/                            Authentication
│   ├── mod.rs
│   ├── authorizer.rs                Authorizer trait + RequestContext
│   ├── default_authorizer.rs        Default implementation
│   ├── pairing.rs                   Pairing request types
│   └── default_pairing_service.rs   Default pairing service
│
├── net/                             Network layer
│   ├── mod.rs
│   ├── server.rs                    TcpListener + axum setup
│   ├── routes.rs                    REST endpoint handlers
│   ├── websocket.rs                 WebSocket pump handlers
│   ├── state.rs                     AppState shared state
│   ├── auth.rs                      Auth middleware
│   └── json.rs                      JSON helpers
│
├── session/                         Session management
│   ├── mod.rs                       PtyProcess trait + exports
│   ├── types.rs                     SessionId, SessionStatus, etc.
│   ├── launch.rs                    LaunchSpec, TerminalSize
│   ├── registry.rs                  SessionRegistry + RunningSession
│   ├── conpty.rs                    Windows ConPTY (portable-pty)
│   ├── posix_pty.rs                 Unix PTY (placeholder)
│   ├── runtime.rs                   Session runtime wrapper
│   ├── lifecycle.rs                 Lifecycle state machine
│   ├── env.rs                       Environment mode types
│   ├── env_file_parser.rs           .env file parsing
│   ├── env_resolver.rs              Environment resolution
│   ├── output_buffer.rs             Ring buffer for output history
│   ├── record.rs                    Session record / snapshot types
│   ├── snapshot.rs                  SessionSnapshot
│   ├── terminal.rs                  Terminal grid / scrollback model
│   └── provider_config.rs           Provider-specific defaults
│
├── service/                         Business logic
│   ├── mod.rs
│   ├── session_manager.rs           Session lifecycle service
│   ├── log_buffer.rs                Structured log ring buffer
│   ├── evidence.rs                  Evidence assembly
│   ├── observation_store.rs         Observation storage
│   └── types.rs                     Service-level types
│
└── store/                           Persistence
    ├── mod.rs
    ├── file_store.rs                JSON file-backed stores
    ├── host_config.rs               HostIdentity configuration
    ├── session_store.rs             Session records persistence
    └── pairing_store.rs             Pairing data persistence
```

---

## 8. Limitations and Risks

### Buffering

- The background reader thread uses an unbounded `mpsc` channel. If the
  WebSocket client is slow, messages accumulate in memory.
- No back-pressure mechanism exists. A disconnected or slow client can
  cause unbounded memory growth.

### Blocking I/O

- `portable-pty`'s `reader.read()` is a blocking call. It runs in a
  dedicated `std::thread`, not a tokio task, to avoid blocking the
  async runtime.
- `writer.write_all()` is also blocking. It is called while holding the
  `RwLock` write guard on the session, which briefly blocks the output
  pump.

### Client disconnects

- When a WebSocket client disconnects, both pumps break and the task
  ends. The PTY session and child process continue running.
- There is no mechanism to terminate a session on client disconnect.
  Sessions persist until explicitly stopped via `POST /sessions/{id}/stop`
  or the child process exits naturally.

### DSR handshake

- If the automatic DSR response in `ConPtyProcess::start()` is removed
  or fails, interactive applications (including cmd.exe) may hang
  silently because they wait for a cursor-position response before
  rendering output.

### Security

- The remote server binds to `0.0.0.0:18086` by default. Without
  TLS and authentication, any network client can create sessions and
  execute arbitrary commands.
- Authentication middleware exists (`net/auth.rs`) but is not wired
  into the route pipeline. All endpoints are currently unauthenticated.
- Exposing a shell (cmd.exe, bash, or Claude CLI) over an unauthenticated
  network endpoint is equivalent to giving remote code execution to
  anyone who can reach the port.

### TUI applications

- Full-screen TUI applications (vim, Claude CLI interactive mode, etc.)
  use alternate screen buffers and complex cursor positioning. These
  work correctly only if the remote client implements a full terminal
  emulator (e.g. xterm.js). Simple text-based clients will see raw
  ANSI escape sequences.

### Platform support

- The Unix PTY implementation (`posix_pty.rs`) is a placeholder. Only
  the Windows ConPTY path is functional.
