# Web Terminal Demo — Setup & Usage Guide

## Prerequisites

| Requirement | Version | Check |
|-------------|---------|-------|
| Rust toolchain | >= 1.75 | `rustc --version` |
| Windows 10/11 | Build 19041+ | Required for ConPTY |
| Web browser | Chrome 90+ / Edge 90+ / Firefox 90+ | With WebSocket support |

## Quick Start

### 1. Build

```bash
cargo build --release
```

The binary is produced at `target/release/pty-claude.exe`.

### 2. Start the server

```bash
# Default ports: admin=18085, remote=18086
cargo run --release -- serve

# With debug logging (shows all input/output bytes)
RUST_LOG=debug cargo run --release -- serve

# Custom ports
cargo run --release -- serve --admin-port 9000 --remote-port 9001

# Custom data directory
cargo run --release -- serve --datadir C:\pty-claude-data
```

You should see:

```
pty-claude v0.2.5 starting...
  Admin:  127.0.0.1:18085
  Remote: 0.0.0.0:18086
  Data:   ...
```

### 3. Open the web terminal

Open your browser and navigate to:

```
http://localhost:18085/terminal
```

### 4. Use the terminal

1. Select a shell from the dropdown (cmd, PowerShell, or bash/WSL)
2. Click **Start Terminal**
3. Type commands — the terminal is fully interactive:
   - `echo hello` — basic output
   - `cls` / `clear` — clear screen
   - `dir` / `ls` — list files
   - Arrow keys, backspace, Ctrl+C all work
4. Click **Stop** to terminate the session

## Architecture

```
 Browser (xterm.js)
     |
     | WebSocket (JSON)
     |
 [axum HTTP server :18085]
     |
     | read() / write()
     |
 [ConPTY (Windows pseudo-terminal)]
     |
     | stdin/stdout
     |
 [cmd.exe / powershell / bash]
```

Data flow for each keystroke:

1. User presses key in browser
2. xterm.js `onData()` fires with the character (e.g., `\r` for Enter)
3. Browser sends `{"type":"input","data":"\r"}` via WebSocket
4. Server parses JSON, writes bytes to ConPTY stdin pipe
5. ConPTY delivers input to the child shell
6. Shell produces output (echo, command result)
7. ConPTY captures output, server reads it via background thread
8. Server sends `{"type":"output","data":"..."}` back to browser
9. xterm.js renders the ANSI escape sequences

## API Reference

### REST Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/terminal` | Web terminal UI |
| POST | `/sessions` | Create a terminal session |
| GET | `/sessions` | List active sessions |
| POST | `/sessions/:id/input` | Send text input |
| POST | `/sessions/:id/stop` | Terminate a session |
| POST | `/sessions/:id/resize` | Resize terminal |

### WebSocket Endpoint

```
ws://localhost:18085/ws/sessions/{session_id}
```

**Client → Server messages:**

```json
{"type": "input", "data": "echo hello\r"}
{"type": "resize", "columns": 120, "rows": 40}
{"type": "stop"}
```

**Server → Client messages:**

```json
{"type": "output", "data": "hello\r\n"}
{"type": "exited", "code": 0}
{"type": "error", "message": "session not found"}
```

## Debugging

### Enable browser-side logging

The web terminal has built-in I/O logging. Open browser DevTools (F12) → Console tab.
You will see entries like:

```
[INP] 5B [65 63 68 6f 20] "echo "
[OUT] 48B [0d 0a 68 65 6c 6c 6f ...] "hello\r\n"
```

### Enable server-side logging

```bash
RUST_LOG=debug cargo run --release -- serve
```

Key log lines to look for:

```
PTY started: pid=12345, size=80x24          # Session created
PTY write: 5 bytes [65 63 68 6f 0d] "echo"  # Input received from browser
PTY read: 48 bytes "hello\r\n"               # Output from shell
Child process exited with code 0              # Shell closed
```

### Common issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| "Failed to create session" | pty-claude.exe not running | Start the server first |
| Terminal shows nothing | Browser can't reach server | Check http://localhost:18085/health |
| Characters don't appear | Server not running with latest fix | Rebuild with `cargo build --release` |
| Screen doesn't clear on `cls` | Old binary without poll_exit fix | Rebuild from source |
| WebSocket disconnects immediately | Session already exited | Click "Start Terminal" again |
| Port already in use | Another instance running | `taskkill /F /IM pty-claude.exe` |

## Running the automated WebSocket test

A Node.js-based test script verifies the full pipeline:

```bash
# Install ws package (already in package.json)
npm install

# Run the test
node -e "
const WebSocket = require('ws');
const http = require('http');

// Create session
const req = http.request('http://localhost:18085/sessions', {
  method: 'POST',
  headers: {'Content-Type': 'application/json'}
}, (res) => {
  let body = '';
  res.on('data', c => body += c);
  res.on('end', () => {
    const {session_id} = JSON.parse(body);
    console.log('Session:', session_id);

    // Connect WebSocket
    const ws = new WebSocket('ws://localhost:18085/ws/sessions/' + session_id);
    let outputCount = 0;

    ws.on('open', () => {
      ws.send(JSON.stringify({type: 'resize', columns: 80, rows: 24}));
      setTimeout(() => {
        ws.send(JSON.stringify({type: 'input', data: 'echo test_pass\r'}));
      }, 300);
      setTimeout(() => { ws.close(); process.exit(0); }, 3000);
    });

    ws.on('message', (raw) => {
      const msg = JSON.parse(raw);
      if (msg.type === 'output') {
        outputCount++;
        if (msg.data.includes('test_pass')) {
          console.log('PASS: echo output received');
        }
      }
    });

    ws.on('close', () => {
      console.log('Output chunks received:', outputCount);
    });
  });
});
req.write(JSON.stringify({provider: 'codex', workspace_root: '.'}));
req.end();
"
```

Expected output:

```
Session: sess_...
PASS: echo output received
Output chunks received: N
```
