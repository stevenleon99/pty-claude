<div align="center">

<img src="doc/image/claude.png" alt="pty-claude" width="700">

# pty-claude

**Your terminal, in any browser.**

Stream real ConPTY / PTY shell sessions over WebSocket to a sleek web UI —
whether you're on localhost or halfway across the planet.

<br>

[![Rust](https://img.shields.io/badge/Rust-1.75+-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Windows%20%7C%20Linux-4A90D9?style=for-the-badge&logo=windows&logoColor=white)](https://github.com)
[![Version](https://img.shields.io/badge/v0.2.5-2EA043?style=for-the-badge&logoColor=white)](https://github.com)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow?style=for-the-badge)](LICENSE)

<br>

[Getting Started](#getting-started) &bull; [Features](#features) &bull; [Screenshots](#screenshots) &bull; [Architecture](#architecture) &bull; [API](#api-reference)

</div>

---

## Why pty-claude?

You need a **real terminal**, not a toy. pty-claude gives you a native shell
(cmd, PowerShell, bash) rendered in the browser with full ANSI color support,
proper cursor handling, and real I/O — all over WebSocket. No plugins, no
extensions, no nonsense.

## Features

| | Feature | Description |
|---|---|---|
| :computer: | **Web Terminal** | xterm.js-powered green-on-black terminal with full ANSI color support |
| :shell: | **Shell Selector** | Switch between cmd, PowerShell, and bash from the toolbar |
| :zap: | **WebSocket Streaming** | Real-time bidirectional I/O with low-latency input handling |
| :lock: | **Password Gate** | Login-protected access before any terminal session starts |
| :iphone: | **PWA Installable** | Add to home screen on desktop or mobile for an app-like experience |
| :globe_with_meridians: | **Cloudflare Tunnel** | One command to expose your terminal over the internet |
| :wrench: | **Dual Ports** | Admin panel on `localhost:18085` &mdash; public terminal on `:18086` |

## Screenshots

<div align="center">
  <img src="doc/image/login.png" alt="Login screen" width="380">
  &nbsp;&nbsp;
  <img src="doc/image/mainpage.png" alt="Terminal" width="380">
</div>

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) 1.75 or later
- On Windows: the MSVC build tools (via Visual Studio Build Tools)
- On Linux: `gcc` and standard development headers

### Build & Run

```bash
# Clone the repo
git clone https://github.com/your-user/pty-claude.git
cd pty-claude

# Build and start
cargo build
cargo run -- serve
```

Open [http://127.0.0.1:18086](http://127.0.0.1:18086) and start typing.

### Expose via Cloudflare Tunnel

```bash
./dev.sh
```

This builds the project, starts the server, and opens a Cloudflare Tunnel —
giving you a public URL to share your terminal instantly.

### CLI Options

```
pty-claude serve [OPTIONS]

  --admin-host <HOST>     Admin bind address   (default: 127.0.0.1)
  --admin-port <PORT>     Admin port           (default: 18085)
  --remote-host <HOST>    Remote bind address  (default: 0.0.0.0)
  --remote-port <PORT>    Remote port          (default: 18086)
  --datadir <DIR>         Data directory path
  --no-discovery          Disable UDP discovery
```

## Architecture

```
 Browser                           Rust Server                     OS
 ───────                           ───────────                     ──
┌──────────┐   WebSocket    ┌───────────────┐   ConPTY/PTY  ┌──────────┐
│ xterm.js │◄─────────────►│  axum + ws    │◄─────────────►│ cmd/bash │
│  web UI  │   JSON I/O     │  :18086       │  stdin/stdout │  ps1     │
└──────────┘                └───────────────┘              └──────────┘

                            ┌───────────────┐
                            │  admin API    │  :18085 (localhost only)
                            └───────────────┘
```

### Source Layout

```
src/
├── main.rs            CLI entry point
├── net/               HTTP server, routes, WebSocket handlers
├── session/           Session management, ConPTY / PTY drivers
├── auth/              Device pairing & authentication
├── store/             File-based persistence
├── service/           Observation store
└── terminal/          Web UI (HTML, CSS, JS, PWA)
```

## API Reference

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/` | Web terminal UI |
| `POST` | `/sessions` | Create terminal session |
| `GET` | `/sessions` | List active sessions |
| `POST` | `/sessions/:id/stop` | Stop a session |
| `POST` | `/sessions/:id/resize` | Resize terminal |
| `WS` | `/ws/sessions/:id` | Terminal I/O stream |
| `WS` | `/ws/overview` | Session overview stream |

## Web UI

The frontend lives in `terminal/` and is built with **zero tooling**:

- **No build step** &mdash; plain HTML, CSS, and JavaScript
- **xterm.js** loaded from CDN for full terminal emulation (cursor, ANSI colors, scrollback)
- **PWA-ready** with `manifest.json` and a service worker for offline install
- **Responsive** design that works on desktop, tablet, and mobile

### Customization Points

| Marker | File | Controls |
|--------|------|----------|
| `[PASSWORD]` | `terminal/terminal.js` | Login password |
| `[WS-URL]` | `terminal/terminal.js` | WebSocket server URL |
| `[THEME]` | `terminal/style.css` | Colors and fonts |

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Runtime | [tokio](https://tokio.rs/) async runtime |
| HTTP | [axum](https://github.com/tokio-rs/axum) web framework |
| Terminal | ConPTY (Windows) / POSIX PTY (Linux) |
| Frontend | [xterm.js](https://xtermjs.org/) |
| Tunnel | [Cloudflare Tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/) |

## License

This project is licensed under the [MIT License](LICENSE).
