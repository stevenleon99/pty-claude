/**
 * terminal.js — ConPTY Remote Terminal
 *
 * Uses xterm.js for proper terminal emulation with correct cursor positioning.
 *
 * CUSTOMIZATION — search for these markers:
 *   [PASSWORD]  hard-coded login password
 *   [WS-URL]    WebSocket server URL
 */

// ═══════════════════════════════════════════════════════════════════════
// CONFIG
// ═══════════════════════════════════════════════════════════════════════

const CONFIG = {
    // [PASSWORD] Hard-coded password — NOT secure
    password: '1234',

    // [WS-URL] Backend server URL (auto-detected from page location)
    serverUrl: `${location.protocol}//${location.host}`,
    wsUrl: `${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}`,

    debug: true,
};

// ═══════════════════════════════════════════════════════════════════════
// STATE
// ═══════════════════════════════════════════════════════════════════════

const S = {
    term: null,
    fit: null,
    ws: null,
    sid: null,
    connected: false,
    authenticated: false,
    resizeTimer: null,
};

// ═══════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════

function $(id) { return document.getElementById(id); }

function log(tag, ...args) {
    if (!CONFIG.debug) return;
    const t = new Date().toISOString().slice(11, 23);
    console.log(`%c[${t}][${tag}]`, 'color:#008800', ...args);
}

function status(cls, text) {
    $('status-dot').className = cls;
    $('status-text').textContent = text;
    $('sl-center').textContent = text;
}

function updateSize() {
    if (!S.term) return;
    $('term-size').textContent = `${S.term.cols}x${S.term.rows}`;
}

// ═══════════════════════════════════════════════════════════════════════
// LOGIN
// ═══════════════════════════════════════════════════════════════════════

function handleLogin(e) {
    if (e) e.preventDefault();
    const input = $('pw-input');
    const box = document.querySelector('.lock-box');

    if (input.value === CONFIG.password) {
        S.authenticated = true;
        input.value = '';
        $('lock-overlay').classList.add('unlocked');
        initTerminal();
    } else {
        $('pw-error').textContent = 'wrong password';
        input.value = '';
        input.focus();
        box.classList.remove('shake');
        box.offsetHeight;
        box.classList.add('shake');
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TERMINAL INIT (xterm.js)
// ═══════════════════════════════════════════════════════════════════════

function initTerminal() {
    if (S.term) return;

    S.term = new Terminal({
        theme: {
            background:             '#000000',
            foreground:             '#00ff00',
            cursor:                 '#00ff00',
            cursorAccent:           '#000000',
            selectionBackground:    'rgba(0,100,0,0.4)',
            selectionForeground:    '#00ff00',
            black:                  '#000000',
            red:                    '#aa0000',
            green:                  '#00ff00',
            yellow:                 '#cccc00',
            blue:                   '#0066ff',
            magenta:                '#cc00cc',
            cyan:                   '#00cccc',
            white:                  '#cccccc',
            brightBlack:            '#444444',
            brightRed:              '#ff4444',
            brightGreen:            '#44ff44',
            brightYellow:           '#ffff44',
            brightBlue:             '#4488ff',
            brightMagenta:          '#ff44ff',
            brightCyan:             '#44ffff',
            brightWhite:            '#ffffff',
        },
        fontFamily:       "'JetBrains Mono', 'Fira Code', 'Cascadia Code', Consolas, monospace",
        fontSize:         15,
        fontWeight:       'normal',
        fontWeightBold:   'bold',
        cursorBlink:      true,
        cursorStyle:      'block',
        scrollback:       10000,
        allowProposedApi: true,
        drawBoldTextInBrightColors: false,
    });

    S.fit = new FitAddon.FitAddon();
    S.term.loadAddon(S.fit);
    S.term.open($('terminal'));
    S.fit.fit();

    // Every keystroke → backend, unmodified
    S.term.onData(data => {
        log('INP', `${data.length}B`);
        if (S.ws?.readyState === WebSocket.OPEN) {
            S.ws.send(JSON.stringify({ type: 'input', data }));
        }
    });

    // Resize handler
    window.addEventListener('resize', () => {
        clearTimeout(S.resizeTimer);
        S.resizeTimer = setTimeout(() => {
            if (S.fit) S.fit.fit();
            updateSize();
            sendResize();
        }, 60);
    });

    updateSize();
    status('off', 'ready');

    // Setup toolbar buttons
    $('btn-start').addEventListener('click', handleStart);
    $('btn-stop').addEventListener('click', stopSession);

    log('INIT', 'xterm ready');
}

// ═══════════════════════════════════════════════════════════════════════
// SESSION API
// ═══════════════════════════════════════════════════════════════════════

async function createSession() {
    const shell = $('shell-select').value;

    const payload = {
        provider: 'codex',
        workspace_root: '.',
        title: shell,
    };

    if (shell === 'powershell') {
        payload.command_argv = ['powershell.exe', '-NoLogo'];
    } else if (shell === 'bash') {
        payload.command_argv = ['bash', '-i'];
    }

    log('API', 'POST /sessions', payload);

    const r = await fetch(`${CONFIG.serverUrl}/sessions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
    });

    if (!r.ok) throw new Error(`server ${r.status}`);
    const d = await r.json();
    log('API', 'created', d.session_id);
    return d.session_id;
}

async function stopSession() {
    if (!S.sid) return;
    try {
        await fetch(`${CONFIG.serverUrl}/sessions/${S.sid}/stop`, { method: 'POST' });
    } catch {}
    disconnect();
}

// ═══════════════════════════════════════════════════════════════════════
// WEBSOCKET
// ═══════════════════════════════════════════════════════════════════════

function connect(sid) {
    S.sid = sid;
    const url = `${CONFIG.wsUrl}/ws/sessions/${sid}`;
    log('WS', '→', url);

    S.ws = new WebSocket(url);

    S.ws.onopen = () => {
        S.connected = true;
        log('WS', 'connected');
        status('ok', 'connected');
        $('sl-left').textContent = sid.slice(0, 16);
        sendResize();
        S.term.focus();
        $('btn-start').disabled = true;
        $('btn-stop').disabled = false;
    };

    S.ws.onmessage = ev => {
        try {
            const m = JSON.parse(ev.data);
            if (m.type === 'output') {
                log('OUT', `${m.data.length}B`);
                S.term.write(m.data);
            } else if (m.type === 'exited') {
                S.term.write('\r\n\x1b[33m── process exited ──\x1b[0m\r\n');
                status('warning', `exited ${m.code ?? '?'}`);
                disconnect();
            } else if (m.type === 'error') {
                S.term.write(`\r\n\x1b[31m${m.message}\x1b[0m\r\n`);
            }
        } catch {
            S.term.write(ev.data);
        }
    };

    S.ws.onerror = () => {
        status('error', 'ws error');
    };

    S.ws.onclose = () => {
        S.connected = false;
        if (!S.sid) return;
        status('off', 'disconnected');
        S.term.write('\r\n\x1b[33m── disconnected ──\x1b[0m\r\n');
        $('btn-start').disabled = false;
        $('btn-stop').disabled = true;
    };
}

function disconnect() {
    S.connected = false;
    S.ws?.close();
    S.ws = null;
    S.sid = null;
    $('btn-start').disabled = false;
    $('btn-stop').disabled = true;
    status('off', 'disconnected');
    $('sl-left').textContent = '';
}

function sendResize() {
    if (!S.term || !S.ws || S.ws.readyState !== WebSocket.OPEN) return;
    const msg = { type: 'resize', columns: S.term.cols, rows: S.term.rows };
    log('WS', 'resize', msg.columns, msg.rows);
    S.ws.send(JSON.stringify(msg));
}

// ═══════════════════════════════════════════════════════════════════════
// START / STOP HANDLERS
// ═══════════════════════════════════════════════════════════════════════

async function handleStart() {
    S.term.reset();
    S.term.write('\x1b[33mconnecting...\x1b[0m\r\n');
    status('warning', 'connecting...');

    try {
        const sid = await createSession();
        connect(sid);
    } catch (e) {
        log('ERR', e);
        S.term.write(`\x1b[31m${e.message}\x1b[0m\r\n`);
        status('error', 'failed');
    }
}

// ═══════════════════════════════════════════════════════════════════════
// INIT
// ═══════════════════════════════════════════════════════════════════════

document.addEventListener('DOMContentLoaded', () => {
    // Service worker
    if ('serviceWorker' in navigator) {
        navigator.serviceWorker.register('service-worker.js')
            .then(reg => log('SW', 'registered', reg.scope))
            .catch(err => log('SW', 'failed', err));
    }

    // Login
    $('pw-btn').addEventListener('click', handleLogin);
    $('pw-input').addEventListener('keydown', e => { if (e.key === 'Enter') handleLogin(e); });

    log('INIT', 'loaded');
});