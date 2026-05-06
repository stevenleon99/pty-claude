//! WebSocket handlers for terminal streaming and session control
//!
//! Implements:
//! - Session I/O WebSocket (ws/sessions/{id}) — full bidirectional terminal bridge
//! - Controller WebSocket (ws/sessions/{id}/controller)
//! - Overview WebSocket (ws/overview)

use axum::{
    extract::{Path, Query, State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, sleep};
use tracing::{debug, info, warn};

use super::state::AppState;

/// Query parameters for session WebSocket.
#[derive(Debug, Deserialize)]
pub struct SessionWsQuery {
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub stream: Option<String>, // "raw" for raw terminal stream
    #[serde(default)]
    pub view_id: Option<String>,
}

/// JSON message from client to session WebSocket.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum SessionClientMessage {
    /// Send text input to the terminal.
    Input { data: String },
    /// Resize the terminal.
    Resize { columns: u16, rows: u16 },
    /// Request control of the session.
    RequestControl { controller_kind: String },
    /// Release control of the session.
    ReleaseControl {},
    /// Stop the session.
    Stop {},
}

/// JSON message from server to client WebSocket.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum SessionServerMessage {
    /// Terminal output data.
    Output { data: String },
    /// Session state update.
    State { status: String },
    /// Terminal resized.
    Resized { columns: u16, rows: u16 },
    /// Session exited.
    Exited { code: Option<i32> },
    /// Error occurred.
    Error { message: String },
    /// Session snapshot update.
    Snapshot {
        snapshot: crate::session::snapshot::SessionSnapshot,
    },
}

/// Maximum output chunk size to send over WebSocket (64 KB).
const MAX_WS_CHUNK: usize = 64 * 1024;

/// Poll interval for ConPTY output (ms).
const POLL_INTERVAL_MS: u64 = 16; // ~60 Hz

// ──── Upgrade handlers ────

/// Upgrade handler for session I/O WebSocket.
pub async fn session_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<SessionWsQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_session_ws(socket, state, session_id, query))
}

/// Upgrade handler for controller WebSocket.
pub async fn controller_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_controller_ws(socket, state, session_id))
}

/// Upgrade handler for overview WebSocket.
pub async fn overview_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_overview_ws(socket, state))
}

// ──── Session I/O handler — the core bridge ────

/// Handle session I/O WebSocket connection.
///
/// This is the main bidirectional bridge:
///   ConPTY stdout → poll_loop → WebSocket → remote client
///   remote client → WebSocket → ConPTY stdin
///
/// Two concurrent tasks:
/// 1. Read from PTY and send to WebSocket (output pump)
/// 2. Read from WebSocket and write to PTY (input pump)
async fn handle_session_ws(
    socket: WebSocket,
    state: AppState,
    session_id: String,
    _query: SessionWsQuery,
) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Look up the session
    let session = state.session_registry.get_session(&session_id).await;
    let session = match session {
        Some(s) => s,
        None => {
            let err = SessionServerMessage::Error {
                message: format!("session '{}' not found", session_id),
            };
            let _ = ws_sender
                .send(Message::Text(serde_json::to_string(&err).unwrap_or_default()))
                .await;
            return;
        }
    };

    // Check if already exited
    {
        let session_read = session.read().await;
        if session_read.is_exited() {
            let msg = SessionServerMessage::Exited {
                code: session_read.exit_code(),
            };
            let _ = ws_sender
                .send(Message::Text(serde_json::to_string(&msg).unwrap_or_default()))
                .await;
            return;
        }
    }

    info!("WebSocket client connected to session {}", session_id);

    // ──── Output pump: poll ConPTY and forward to WebSocket ────
    let session_output = session.clone();
    let session_id_out = session_id.clone();
    let output_handle = tokio::spawn(async move {
        loop {
            let read_result = {
                let mut session = session_output.write().await;

                // Fast path: if already exited, stop immediately
                if session.is_exited() {
                    break;
                }

                // Read ALL available output from the PTY.
                // The updated read() drains the channel completely.
                let result = session.read(0);

                if result.closed {
                    // Output pipe closed — process has exited.
                    // poll_exit() will pick up the exit code via child handle.
                    session.poll_exit();
                    break;
                }

                result
            };

            if read_result.data.is_empty() {
                sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
                continue;
            }

            // Send output to WebSocket
            let text = String::from_utf8_lossy(&read_result.data).to_string();
            let msg = SessionServerMessage::Output { data: text };
            let json = serde_json::to_string(&msg).unwrap_or_default();
            if ws_sender.send(Message::Text(json)).await.is_err() {
                debug!("WebSocket send failed for session {}, client disconnected", session_id_out);
                break;
            }
        }
    });

    // ──── Input pump: receive from WebSocket and write to ConPTY ────
    let session_input = session.clone();
    let session_id_in = session_id.clone();
    let input_handle = tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // Try JSON command first
                    if let Ok(cmd) = serde_json::from_str::<SessionClientMessage>(&text) {
                        match cmd {
                            SessionClientMessage::Input { data } => {
                                let hex_preview: String = data.bytes()
                                    .take(32)
                                    .map(|b| format!("{:02x}", b))
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                debug!(
                                    "WS input session {}: {} bytes [{}] {:?}",
                                    session_id_in, data.len(), hex_preview, &data[..data.len().min(64)]
                                );
                                let mut s = session_input.write().await;
                                if !s.write(data.as_bytes()) {
                                    warn!("Failed to write input to session {}", session_id_in);
                                }
                            }
                            SessionClientMessage::Resize { columns, rows } => {
                                debug!("WS resize session {} to {}x{}", session_id_in, columns, rows);
                                let mut s = session_input.write().await;
                                let size = crate::session::launch::TerminalSize { columns, rows };
                                if s.resize(size) {
                                    debug!("Resized session {} to {}x{}", session_id_in, columns, rows);
                                }
                            }
                            SessionClientMessage::Stop {} => {
                                info!("WS stop session {}", session_id_in);
                                let mut s = session_input.write().await;
                                s.terminate();
                                break;
                            }
                            SessionClientMessage::RequestControl { .. } => {
                                // TODO: Implement control flow
                            }
                            SessionClientMessage::ReleaseControl {} => {
                                // TODO: Implement control flow
                            }
                        }
                    } else {
                        // Raw text input — send directly to PTY
                        debug!("WS raw input session {}: {} bytes", session_id_in, text.len());
                        let mut s = session_input.write().await;
                        s.write(text.as_bytes());
                    }
                }
                Ok(Message::Binary(data)) => {
                    // Raw binary input — send directly to PTY
                    debug!("WS binary input session {}: {} bytes", session_id_in, data.len());
                    let mut s = session_input.write().await;
                    s.write(&data);
                }
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    warn!("WebSocket error for session {}: {}", session_id_in, e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for either pump to finish
    tokio::select! {
        _ = output_handle => {
            debug!("Output pump finished for session {}", session_id);
        }
        _ = input_handle => {
            debug!("Input pump finished for session {}", session_id);
        }
    }

    // Send exit notification if the process died
    let session_read = session.read().await;
    if session_read.is_exited() {
        let exit_msg = SessionServerMessage::Exited {
            code: session_read.exit_code(),
        };
        // Best-effort send — client might already be gone
        let _json = serde_json::to_string(&exit_msg).unwrap_or_default();
        // ws_sender might be consumed by output_handle, so we just log
        debug!("Session {} exited with code {:?}", session_id, session_read.exit_code());
    }

    info!("WebSocket client disconnected from session {}", session_id);
}

// ──── Controller handler ────

async fn handle_controller_ws(
    socket: WebSocket,
    _state: AppState,
    _session_id: String,
) {
    let (_sender, mut receiver) = socket.split();

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(_text)) => {
                // TODO: Handle controller commands (request/release control, etc.)
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }
}

// ──── Overview handler ────

async fn handle_overview_ws(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Send periodic session list updates
    let update_handle = tokio::spawn(async move {
        loop {
            let sessions = state.session_registry.list_sessions().await;
            let msg = serde_json::json!({
                "type": "overview",
                "sessions": sessions,
                "count": sessions.len(),
            });
            if sender
                .send(Message::Text(serde_json::to_string(&msg).unwrap_or_default()))
                .await
                .is_err()
            {
                break;
            }
            sleep(Duration::from_secs(2)).await;
        }
    });

    // Wait for client to disconnect
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    update_handle.abort();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_client_message_parse_input() {
        let msg: SessionClientMessage = serde_json::from_str(
            r#"{"type":"input","data":"ls -la\n"}"#
        ).unwrap();
        match msg {
            SessionClientMessage::Input { data } => assert_eq!(data, "ls -la\n"),
            _ => panic!("Expected Input variant"),
        }
    }

    #[test]
    fn test_session_client_message_parse_resize() {
        let msg: SessionClientMessage = serde_json::from_str(
            r#"{"type":"resize","columns":120,"rows":40}"#
        ).unwrap();
        match msg {
            SessionClientMessage::Resize { columns, rows } => {
                assert_eq!(columns, 120);
                assert_eq!(rows, 40);
            }
            _ => panic!("Expected Resize variant"),
        }
    }

    #[test]
    fn test_session_server_message_serialize_output() {
        let msg = SessionServerMessage::Output {
            data: "Hello, World!\n".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"output\""));
        assert!(json.contains("Hello, World!"));
    }

    #[test]
    fn test_session_server_message_serialize_exited() {
        let msg = SessionServerMessage::Exited { code: Some(0) };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"exited\""));
        assert!(json.contains("\"code\":0"));
    }

    #[test]
    fn test_session_server_message_serialize_state() {
        let msg = SessionServerMessage::State { status: "running".to_string() };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"state\""));
        assert!(json.contains("running"));
    }

    #[test]
    fn test_session_server_message_serialize_resized() {
        let msg = SessionServerMessage::Resized { columns: 80, rows: 24 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"resized\""));
    }
}
