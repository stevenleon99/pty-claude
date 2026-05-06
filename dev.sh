#!/usr/bin/env bash
set -euo pipefail

# ─── pty-claude dev launcher with Cloudflare Tunnel ───

# Colors
GRN='\033[0;32m'
CYN='\033[0;36m'
RED='\033[0;31m'
RST='\033[0m'

log()  { printf "${GRN}[pty-claude]${RST} %s\n" "$*"; }
info() { printf "${CYN}[tunnel]${RST}   %s\n" "$*"; }
die()  { printf "${RED}[error]${RST}    %s\n" "$*" >&2; exit 1; }

# ─── Cleanup on exit ───
PID_SERVER=0
PID_TUNNEL=0

cleanup() {
    echo ""
    log "shutting down..."
    [ "$PID_TUNNEL" -ne 0 ]  && kill "$PID_TUNNEL"  2>/dev/null && log "tunnel stopped"
    [ "$PID_SERVER" -ne 0 ]  && kill "$PID_SERVER"  2>/dev/null && log "server stopped"
    wait 2>/dev/null
    log "bye"
}
trap cleanup EXIT INT TERM

# ─── Build ───
log "building..."
cargo build || die "build failed"
log "build ok"

# ─── Start server ───
REMOTE_PORT="${REMOTE_PORT:-18086}"
ADMIN_PORT="${ADMIN_PORT:-18085}"

log "starting pty-claude on :${REMOTE_PORT} (admin :${ADMIN_PORT})"
cargo run -- serve \
    --remote-port "$REMOTE_PORT" \
    --admin-port  "$ADMIN_PORT" \
    &
PID_SERVER=$!

# Wait for server to be ready
for i in $(seq 1 30); do
    if curl -sf "http://127.0.0.1:${REMOTE_PORT}/" >/dev/null 2>&1; then
        break
    fi
    sleep 0.5
done

if ! curl -sf "http://127.0.0.1:${REMOTE_PORT}/" >/dev/null 2>&1; then
    die "server didn't start on :${REMOTE_PORT}"
fi

log "server ready at http://127.0.0.1:${REMOTE_PORT}"

# ─── Start Cloudflare Tunnel ───
if command -v cloudflared &>/dev/null; then
    log "starting cloudflare tunnel..."
    cloudflared tunnel run 8458aafc-1101-4dc6-8eca-b069a983cc2b &
    PID_TUNNEL=$!

    # Wait for tunnel to print the URL
    sleep 3
    info "tunnel active — check above for your public URL"
else
    die "cloudflared not found. Install it: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/"
fi

# ─── Wait ───
log "press Ctrl+C to stop"
wait
