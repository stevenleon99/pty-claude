# Manual Test Instructions

Step-by-step guide for verifying that a remote terminal client can see
and interact with Claude CLI running inside a ConPTY session through
the Sentrits WebSocket bridge.

---

## 1. Test Objective

Verify that:

1. The daemon starts and accepts HTTP/WebSocket connections.
2. A ConPTY session can be created via the REST API.
3. A WebSocket client receives real-time terminal output from the
   session.
4. Input sent over the WebSocket reaches the child process.
5. Claude CLI output is visible through the WebSocket.
6. Terminal resize is forwarded correctly.

---

## 2. Prerequisites

| Requirement | Details |
|---|---|
| Operating system | Windows 10 1809+ or Windows 11 |
| Rust toolchain | `rustup` with stable MSVC target (`x86_64-pc-windows-msvc`) |
| Node.js | v18+ installed and in PATH |
| Claude CLI | `npm install -g @anthropic-ai/claude-code`; verify with `claude --version` |
| Network client | One of: Node.js with `ws` package, `websocat`, or a browser terminal |
| Git Bash or WSL | Required for shell commands in this guide |

---

## 3. Build and Start the Daemon

### 3.1 Build

```bash
cargo build
```

The binary is at `target/debug/sentrits.exe`.

### 3.2 Start the server

```bash
./target/debug/sentrits.exe serve
```

Expected output:

```
Sentrits v0.2.5 starting...
  Admin:  127.0.0.1:18085
  Remote: 0.0.0.0:18086
  Data:   C:\Users\...\AppData\Local\sentrits
```

### 3.3 Verify health

```bash
curl -s http://127.0.0.1:18085/health
```

Expected: `ok`

---

## 4. Test 1 — Create a cmd.exe Session

### 4.1 Create session

```bash
curl -s -X POST http://127.0.0.1:18085/sessions \
  -H "Content-Type: application/json" \
  -d '{"provider":"codex","workspace_root":".","command_argv":["cmd.exe"]}'
```

Expected response (session ID and PID will vary):

```json
{ "session_id": "sess_b08o7sboqrk0fdzw", "pid": 111732 }
```

Record the `session_id` for the next steps.

### 4.2 List sessions

```bash
curl -s http://127.0.0.1:18085/sessions
```

Expected: JSON array containing the session ID from step 4.1.

---

## 5. Test 2 — Bidirectional Terminal I/O via WebSocket

This test verifies that input sent over WebSocket reaches cmd.exe and
output from cmd.exe is sent back.

### 5.1 Install WebSocket client

```bash
npm install ws
```

### 5.2 Create test script

Save as `test_ws.js`:

```javascript
const WebSocket = require('ws');

const sessionId = process.argv[2];
if (!sessionId) {
  console.error('Usage: node test_ws.js <session_id>');
  process.exit(1);
}

const url = `ws://127.0.0.1:18085/ws/sessions/${sessionId}`;
console.log(`Connecting to ${url}`);
const ws = new WebSocket(url);
let outputBuffer = '';

ws.on('open', () => {
  console.log('WebSocket connected');

  // Wait for ConPTY to initialize and send the Windows banner
  setTimeout(() => {
    console.log('Sending: echo hello_websocket_test');
    ws.send(JSON.stringify({
      type: 'input',
      data: 'echo hello_websocket_test\r\n'
    }));
  }, 2000);
});

ws.on('message', (data) => {
  const text = data.toString();
  try {
    const msg = JSON.parse(text);
    if (msg.type === 'output') {
      outputBuffer += msg.data;
      const visible = msg.data
        .replace(/\x1b\][^\x07]*\x07/g, '')
        .replace(/\x1b\[[^a-zA-Z]*[a-zA-Z]/g, '')
        .replace(/\r/g, '');
      if (visible.trim()) process.stdout.write(visible);
    } else if (msg.type === 'exited') {
      console.log(`\n[EXITED] code=${msg.code}`);
    }
  } catch (e) {
    console.log('[RAW]', text);
  }
});

ws.on('error', (err) => console.error('WebSocket error:', err.message));

ws.on('close', () => {
  console.log('\n\nWebSocket closed');
  if (outputBuffer.includes('hello_websocket_test')) {
    console.log('=== PASS: Bidirectional I/O verified ===');
    process.exit(0);
  } else {
    console.log('=== FAIL: Output not received ===');
    process.exit(1);
  }
});

setTimeout(() => {
  console.log('\nTimeout - closing');
  ws.close();
}, 10000);
```

### 5.3 Run the test

```bash
node test_ws.js <session_id_from_step_4.1>
```

Expected output:

```
Connecting to ws://127.0.0.1:18085/ws/sessions/sess_...
WebSocket connected
Microsoft Windows [Version 10.0.26200.8328]
(c) Microsoft Corporation. All rights reserved.
C:\...>hello_websocket_test
C:\...>
=== PASS: Bidirectional I/O verified ===
```

---

## 6. Test 3 — Terminal Resize

### 6.1 Send a resize command

```bash
curl -s -X POST http://127.0.0.1:18085/sessions/<session_id>/resize \
  -H "Content-Type: application/json" \
  -d '{"terminal_size":{"columns":120,"rows":40}}'
```

Expected: `{"status":"resized"}`

### 6.2 Verify via WebSocket

Connect to the session WebSocket and check that the terminal now wraps
at 120 columns.

---

## 7. Test 4 — Claude CLI `--version`

This test verifies that Claude CLI can run inside the ConPTY session
and its output reaches the WebSocket client.

### 7.1 Create a session running Claude CLI

Claude CLI is an npm wrapper script. To avoid batch-file escaping
issues, launch `node` directly with the CLI entry point.

First, find your npm global prefix:

```bash
npm prefix -g
```

Then create the session:

```bash
NPM_PREFIX=$(npm prefix -g | sed 's|\\|/|g')

curl -s -X POST http://127.0.0.1:18085/sessions \
  -H "Content-Type: application/json" \
  -d "{\"provider\":\"claude\",\"workspace_root\":\".\",\"command_argv\":[\"node\",\"${NPM_PREFIX}/node_modules/@anthropic-ai/claude-code/cli.js\",\"--version\"]}"
```

Expected response:

```json
{ "session_id": "sess_...", "pid": ... }
```

### 7.2 Read the version output via WebSocket

Save as `test_ws_version.js`:

```javascript
const WebSocket = require('ws');
const sessionId = process.argv[2];
const ws = new WebSocket(`ws://127.0.0.1:18085/ws/sessions/${sessionId}`);
let outputBuffer = '';

ws.on('open', () => console.log('Connected'));
ws.on('message', (data) => {
  try {
    const msg = JSON.parse(data.toString());
    if (msg.type === 'output') {
      outputBuffer += msg.data;
    } else if (msg.type === 'exited') {
      console.log(`Exited: code=${msg.code}`);
    }
  } catch (e) {}
});
ws.on('close', () => {
  const clean = outputBuffer.replace(/\x1b\[[^a-zA-Z]*[a-zA-Z]/g, '');
  console.log('Output:', clean.trim());
  if (clean.includes('Claude Code')) {
    console.log('=== PASS ===');
    process.exit(0);
  } else {
    console.log('=== FAIL ===');
    process.exit(1);
  }
});
setTimeout(() => ws.close(), 5000);
```

Run:

```bash
node test_ws_version.js <session_id>
```

Expected output:

```
Connected
Output: 2.1.94 (Claude Code)
=== PASS ===
```

---

## 8. Test 5 — Interactive Claude CLI Session

This test connects to a full interactive Claude CLI TUI session.

> **Note:** Claude CLI uses a full-screen TUI (Ink/React). The output
> contains ANSI escape sequences for cursor positioning, colors, and
> alternate screen buffers. A plain-text WebSocket client will see raw
> escape sequences, not rendered content. Use xterm.js or a similar
> terminal emulator on the client side for readable output.

### 8.1 Create an interactive session

```bash
NPM_PREFIX=$(npm prefix -g | sed 's|\\|/|g')

curl -s -X POST http://127.0.0.1:18085/sessions \
  -H "Content-Type: application/json" \
  -d "{\"provider\":\"claude\",\"workspace_root\":\".\",\"command_argv\":[\"node\",\"${NPM_PREFIX}/node_modules/@anthropic-ai/claude-code/cli.js\"]}"
```

### 8.2 Connect and observe

Connect with any WebSocket client:

```bash
node -e "
  const WebSocket = require('ws');
  const ws = new WebSocket('ws://127.0.0.1:18085/ws/sessions/<session_id>');
  ws.on('open', () => console.log('Connected'));
  ws.on('message', (d) => {
    const msg = JSON.parse(d.toString());
    if (msg.type === 'output') process.stdout.write(msg.data);
  });
"
```

You should see ANSI escape sequences — these are the TUI rendering
commands from Claude CLI. If you pipe them into a terminal emulator
(xterm.js, Windows Terminal via a custom bridge), you will see the
full Claude Code interactive interface.

### 8.3 Send input

From the same WebSocket, send a message:

```json
{ "type": "input", "data": "What is 2+2?\n" }
```

The TUI should update with Claude's response.

---

## 9. Test 6 — Session Lifecycle

### 9.1 Stop a session

```bash
curl -s -X POST http://127.0.0.1:18085/sessions/<session_id>/stop
```

Expected: `{"status":"stopped"}`

### 9.2 Verify session is gone

```bash
curl -s http://127.0.0.1:18085/sessions
```

The stopped session should no longer appear (or show as exited).

---

## 10. Troubleshooting

### No output from cmd.exe

ConPTY sends a Device Status Report (`\x1b[6n`) during initialization.
If the automatic DSR response in `ConPtyProcess::start()` fails, the
child process hangs. Check that the `portable-pty` writer is working
by looking for the DSR response in the server logs:

```
RUST_LOG=debug ./target/debug/sentrits.exe serve
```

### `claude.cmd` produces no output

The npm `.cmd` wrapper uses complex batch escaping that does not work
reliably inside ConPTY. Launch `node` directly with the CLI entry point
as shown in tests 4 and 5.

### Path backslashes are consumed

When sending Windows paths through the WebSocket, use forward slashes
(`/`) instead of backslashes (`\`). ConPTY interprets `\n`, `\r`, `\t`
as control characters in the input stream.

### WebSocket connects but shows no data

- Verify the session is still running: `curl -s http://127.0.0.1:18085/sessions`
- If the child process exited before the WebSocket connected, you will
  receive only an `exited` message.
- For one-shot commands (`--version`), connect the WebSocket immediately
  after creating the session.

### `spawn_command failed: %1 is not a valid Win32 application`

The specified executable is not a native Windows binary (e.g. a POSIX
shell script). Use `cmd.exe` or `node` as the executable instead.

---

## 11. Test Results Summary Template

| Test | Description | Pass / Fail | Notes |
|------|-------------|-------------|-------|
| 1 | Create cmd.exe session | | |
| 2 | Bidirectional WebSocket I/O | | |
| 3 | Terminal resize | | |
| 4 | Claude CLI `--version` | | |
| 5 | Interactive Claude CLI | | |
| 6 | Session stop and cleanup | | |
