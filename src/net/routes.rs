//! HTTP routes and handlers
//!
//! Implements all REST endpoints for:
//! - Health check
//! - Pairing workflow
//! - Session management
//! - Session operations (input, stop, resize, snapshot, tail)
//! - Evidence retrieval
//! - Host configuration
//! - Observations

#![allow(dead_code)]

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::state::AppState;
use super::websocket::{overview_ws, controller_ws, session_ws};
use crate::session::launch::LaunchSpec;
use crate::session::types::{ProviderType, SessionId};
use crate::session::provider_config::ProviderConfig;

/// Build the main API router.
pub fn build_api_router(state: AppState) -> Router {
    Router::new()
        // Health
        .route("/health", get(health_check))

        // Terminal auth
        .route("/auth/login", post(auth_login))

        // Pairing
        .route("/pairing/request", post(pairing_request))
        .route("/pairing/pending", get(pairing_pending))
        .route("/pairing/approve", post(pairing_approve))
        .route("/pairing/reject", post(pairing_reject))

        // Sessions
        .route("/sessions", get(list_sessions).post(create_session))
        .route("/sessions/clear-inactive", post(clear_inactive_sessions))
        .route("/log-sessions", post(create_log_session))

        // Session operations
        .route("/sessions/:id/snapshot", get(session_snapshot))
        .route("/sessions/:id/input", post(session_input))
        .route("/sessions/:id/stop", post(session_stop))
        .route("/sessions/:id/resize", post(session_resize))
        .route("/sessions/:id/groups", post(session_groups))
        .route("/sessions/:id/tail", get(session_tail))
        .route("/sessions/:id/env", get(session_env))
        .route("/sessions/:id/file", get(session_file))

        // Evidence
        .route("/sessions/:id/evidence/tail", get(evidence_tail))
        .route("/sessions/:id/evidence/search", get(evidence_search))
        .route("/sessions/:id/evidence/range", get(evidence_range))
        .route("/sessions/:id/evidence/context", get(evidence_context))

        // Host management
        .route("/host/info", get(host_info))
        .route("/host/config", post(host_config))
        .route("/host/records", get(host_records))
        .route("/host/tls/certificate", get(host_tls_certificate))
        .route("/host/logs", get(host_logs))
        .route("/host/local-token", get(host_local_token))
        .route("/host/sessions", get(host_sessions).post(host_create_session))
        .route("/host/clients", get(host_clients))
        .route("/host/trusted-devices", get(host_trusted_devices))
        .route("/host/sessions/clear-inactive", post(host_clear_inactive))

        // Observations
        .route("/observations", get(list_observations))

        // Web terminal static files
        .route("/", get(terminal_index))
        .route("/terminal", get(terminal_index))
        .route("/style.css", get(terminal_css))
        .route("/terminal.js", get(terminal_js))
        .route("/manifest.json", get(terminal_manifest))
        .route("/service-worker.js", get(terminal_service_worker))

        // WebSocket endpoints
        .route("/ws/sessions/:id", get(session_ws))
        .route("/ws/sessions/:id/controller", get(controller_ws))
        .route("/ws/overview", get(overview_ws))

        .with_state(state)
}

// --- Health Check ---

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "ok\n")
}

// --- Terminal Auth ---

#[derive(Debug, Deserialize)]
struct LoginPayload {
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    ok: bool,
}

async fn auth_login(
    State(state): State<AppState>,
    Json(payload): Json<LoginPayload>,
) -> impl IntoResponse {
    let ok = payload.password == state.terminal_password;
    (StatusCode::OK, Json(LoginResponse { ok }))
}

// --- Pairing Handlers ---

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PairingRequestPayload {
    device_name: String,
    #[serde(default)]
    device_type: String,
}

#[derive(Debug, Serialize)]
struct PairingRequestResponse {
    pairing_id: String,
    code: String,
    expires_at_unix_ms: i64,
}

async fn pairing_request(
    State(_state): State<AppState>,
    Json(_payload): Json<PairingRequestPayload>,
) -> impl IntoResponse {
    // TODO: Call pairing_service.start_pairing()
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

async fn pairing_pending(State(_state): State<AppState>) -> impl IntoResponse {
    // TODO: Call pairing_service.pending_requests()
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

#[derive(Debug, Deserialize)]
struct PairingApprovePayload {
    pairing_id: String,
    code: String,
}

async fn pairing_approve(
    State(_state): State<AppState>,
    Json(_payload): Json<PairingApprovePayload>,
) -> impl IntoResponse {
    // TODO: Call pairing_service.approve_pairing()
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

#[derive(Debug, Deserialize)]
struct PairingRejectPayload {
    pairing_id: String,
}

async fn pairing_reject(
    State(_state): State<AppState>,
    Json(_payload): Json<PairingRejectPayload>,
) -> impl IntoResponse {
    // TODO: Call pairing_service.reject_pairing()
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

// --- Session Handlers ---

#[derive(Debug, Deserialize)]
struct ListSessionsQuery {
    #[serde(default)]
    include_clients: bool,
}

async fn list_sessions(
    State(state): State<AppState>,
    Query(_query): Query<ListSessionsQuery>,
) -> impl IntoResponse {
    let ids = state.session_registry.list_sessions().await;
    let sessions: Vec<serde_json::Value> = ids.into_iter().map(|id| {
        serde_json::json!({ "session_id": id, "status": "running" })
    }).collect();
    (StatusCode::OK, Json(serde_json::json!({ "sessions": sessions })))
}

#[derive(Debug, Deserialize)]
struct CreateSessionPayload {
    provider: String,
    workspace_root: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    command_argv: Vec<String>,
    #[serde(default)]
    command_shell: Option<String>,
    #[serde(default)]
    group_tags: Vec<String>,
    #[serde(default)]
    record_id: Option<String>,
    #[serde(default)]
    env_mode: Option<String>,
    #[serde(default)]
    environment_overrides: std::collections::HashMap<String, String>,
    #[serde(default)]
    env_file_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateSessionResponse {
    session_id: String,
    pid: u32,
}

async fn create_session(
    State(state): State<AppState>,
    Json(payload): Json<CreateSessionPayload>,
) -> impl IntoResponse {
    // Generate session ID
    let session_id = SessionId::generate();
    let id_str = session_id.as_str().to_string();

    // Parse provider type
    let provider = match payload.provider.as_str() {
        "codex" => ProviderType::Codex,
        "claude" => ProviderType::Claude,
        _ => ProviderType::Codex,
    };

    let _provider_config = ProviderConfig::default_for(provider);

    // Build command — use provided argv or default shell
    let mut argv = payload.command_argv.clone();
    let executable = if argv.is_empty() {
        // Default: cmd.exe on Windows, /bin/sh on Unix
        #[cfg(windows)]
        { "cmd.exe".to_string() }
        #[cfg(not(windows))]
        { "/bin/sh".to_string() }
    } else {
        argv.remove(0)
    };

    // Build launch spec
    let mut effective_env = crate::session::env::EffectiveEnvironment::default();
    // Pass any environment overrides from the client (e.g., ANTHROPIC_AUTH_TOKEN)
    effective_env.overrides = payload.environment_overrides;

    let spec = LaunchSpec {
        provider,
        executable,
        arguments: argv,
        effective_environment: effective_env,
        working_directory: if payload.workspace_root.is_empty() {
            ".".to_string()
        } else {
            payload.workspace_root.clone()
        },
        terminal_size: crate::session::launch::TerminalSize::default(),
    };

    info!("Creating session {} with provider {:?}", id_str, provider);

    // Create session via registry with ConPTY factory
    match state.session_registry.create_session(
        id_str.clone(),
        &spec,
        || {
            #[cfg(windows)]
            {
                Box::new(crate::session::conpty::ConPtyProcess::new())
            }
            #[cfg(not(windows))]
            {
                unimplemented!("Unix PTY not yet implemented in this branch")
            }
        },
    ).await {
        Ok(pid) => {
            let response = CreateSessionResponse {
                session_id: id_str,
                pid: pid as u32,
            };
            (StatusCode::CREATED, Json(serde_json::to_value(response).unwrap()))
        }
        Err(e) => {
            debug!("Failed to create session: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e})))
        }
    }
}

async fn create_log_session(
    State(_state): State<AppState>,
    Json(_payload): Json<CreateSessionPayload>,
) -> impl IntoResponse {
    // TODO: Call session_manager.create_log_session()
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

#[derive(Debug, Serialize)]
struct ClearInactiveResponse {
    removed_count: usize,
}

async fn clear_inactive_sessions(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    // TODO: Remove inactive sessions
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

// --- Session Operations ---

#[derive(Debug, Deserialize)]
struct SessionSnapshotQuery {
    #[serde(default)]
    view_id: Option<String>,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
}

async fn session_snapshot(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    Query(_query): Query<SessionSnapshotQuery>,
) -> impl IntoResponse {
    // TODO: Get session snapshot from session_manager
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

#[derive(Debug, Deserialize)]
struct SessionInputPayload {
    data: String,
}

async fn session_input(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<SessionInputPayload>,
) -> impl IntoResponse {
    match state.session_registry.get_session(&id).await {
        Some(session) => {
            let mut s = session.write().await;
            if s.write(payload.data.as_bytes()) {
                (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
            } else {
                (StatusCode::GONE, Json(serde_json::json!({"error": "session exited"})))
            }
        }
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "session not found"}))),
    }
}

async fn session_stop(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.session_registry.get_session(&id).await {
        Some(session) => {
            let mut s = session.write().await;
            s.terminate();
            (StatusCode::OK, Json(serde_json::json!({"status": "stopped"})))
        }
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "session not found"}))),
    }
}

#[derive(Debug, Deserialize)]
struct SessionResizePayload {
    terminal_size: TerminalSizePayload,
}

#[derive(Debug, Deserialize)]
struct TerminalSizePayload {
    columns: u16,
    rows: u16,
}

async fn session_resize(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<SessionResizePayload>,
) -> impl IntoResponse {
    match state.session_registry.get_session(&id).await {
        Some(session) => {
            let mut s = session.write().await;
            let size = crate::session::launch::TerminalSize {
                columns: payload.terminal_size.columns,
                rows: payload.terminal_size.rows,
            };
            if s.resize(size) {
                (StatusCode::OK, Json(serde_json::json!({"status": "resized"})))
            } else {
                (StatusCode::GONE, Json(serde_json::json!({"error": "session exited"})))
            }
        }
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "session not found"}))),
    }
}

#[derive(Debug, Deserialize)]
struct SessionGroupsPayload {
    mode: String, // "Add" or "Remove"
    tags: Vec<String>,
}

async fn session_groups(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    Json(_payload): Json<SessionGroupsPayload>,
) -> impl IntoResponse {
    // TODO: Update session group tags
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

#[derive(Debug, Deserialize)]
struct SessionTailQuery {
    #[serde(default = "default_tail_bytes")]
    bytes: usize,
}

fn default_tail_bytes() -> usize { 4096 }

async fn session_tail(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    Query(_query): Query<SessionTailQuery>,
) -> impl IntoResponse {
    // TODO: Get recent terminal tail
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

async fn session_env(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> impl IntoResponse {
    // TODO: Get session environment
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

#[derive(Debug, Deserialize)]
struct SessionFileQuery {
    path: String,
    #[serde(default = "default_file_bytes")]
    bytes: usize,
}

fn default_file_bytes() -> usize { 1024 }

async fn session_file(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    Query(_query): Query<SessionFileQuery>,
) -> impl IntoResponse {
    // TODO: Read file from session workspace
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

// --- Evidence Handlers ---

#[derive(Debug, Deserialize)]
struct EvidenceTailQuery {
    #[serde(default = "default_evidence_lines")]
    lines: usize,
}

fn default_evidence_lines() -> usize { 100 }

async fn evidence_tail(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    Query(_query): Query<EvidenceTailQuery>,
) -> impl IntoResponse {
    // TODO: Get evidence tail
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

#[derive(Debug, Deserialize)]
struct EvidenceSearchQuery {
    query: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
}

fn default_search_limit() -> usize { 50 }

async fn evidence_search(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    Query(_query): Query<EvidenceSearchQuery>,
) -> impl IntoResponse {
    // TODO: Search evidence
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

#[derive(Debug, Deserialize)]
struct EvidenceRangeQuery {
    #[serde(default)]
    start: Option<u64>,
    #[serde(default)]
    end: Option<u64>,
    #[serde(default = "default_range_limit")]
    limit: usize,
}

fn default_range_limit() -> usize { 100 }

async fn evidence_range(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    Query(_query): Query<EvidenceRangeQuery>,
) -> impl IntoResponse {
    // TODO: Get evidence range
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

#[derive(Debug, Deserialize)]
struct EvidenceContextQuery {
    revision: u64,
    #[serde(default = "default_context_before")]
    before: usize,
    #[serde(default = "default_context_after")]
    after: usize,
}

fn default_context_before() -> usize { 3 }
fn default_context_after() -> usize { 3 }

async fn evidence_context(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
    Query(_query): Query<EvidenceContextQuery>,
) -> impl IntoResponse {
    // TODO: Get evidence context
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

// --- Host Handlers ---

#[derive(Debug, Serialize)]
struct HostInfoResponse {
    host_id: String,
    display_name: String,
    admin_host: String,
    admin_port: u16,
    remote_host: String,
    remote_port: u16,
    remote_tls: bool,
    provider_types: Vec<String>,
}

async fn host_info(State(_state): State<AppState>) -> impl IntoResponse {
    // TODO: Get host identity from host_config_store
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

#[derive(Debug, Deserialize)]
struct HostConfigPayload {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    admin_host: Option<String>,
    #[serde(default)]
    admin_port: Option<u16>,
    #[serde(default)]
    remote_host: Option<String>,
    #[serde(default)]
    remote_port: Option<u16>,
    #[serde(default)]
    codex_command: Option<Vec<String>>,
    #[serde(default)]
    claude_command: Option<Vec<String>>,
}

async fn host_config(
    State(_state): State<AppState>,
    Json(_payload): Json<HostConfigPayload>,
) -> impl IntoResponse {
    // TODO: Update host config
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

async fn host_records(State(_state): State<AppState>) -> impl IntoResponse {
    // TODO: Get launch records
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

async fn host_tls_certificate(State(_state): State<AppState>) -> impl IntoResponse {
    // TODO: Get TLS certificate info
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

async fn host_logs(State(_state): State<AppState>) -> impl IntoResponse {
    // TODO: Get server logs
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

#[derive(Debug, Serialize)]
struct LocalTokenResponse {
    token: String,
    expires_at_unix_ms: i64,
}

async fn host_local_token(State(_state): State<AppState>) -> impl IntoResponse {
    // TODO: Generate local browser token
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

async fn host_sessions(State(_state): State<AppState>) -> impl IntoResponse {
    // TODO: List all sessions (admin view)
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

async fn host_create_session(
    State(_state): State<AppState>,
    Json(_payload): Json<CreateSessionPayload>,
) -> impl IntoResponse {
    // TODO: Create session (admin)
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

async fn host_clients(State(_state): State<AppState>) -> impl IntoResponse {
    // TODO: List attached clients
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

async fn host_trusted_devices(State(_state): State<AppState>) -> impl IntoResponse {
    // TODO: List trusted devices (pairings)
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

async fn host_clear_inactive(State(_state): State<AppState>) -> impl IntoResponse {
    // TODO: Clear inactive sessions (admin)
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

// --- Observations Handler ---

#[derive(Debug, Deserialize)]
struct ObservationsQuery {
    #[serde(default = "default_observation_limit")]
    limit: usize,
}

fn default_observation_limit() -> usize { 100 }

async fn list_observations(
    State(_state): State<AppState>,
    Query(_query): Query<ObservationsQuery>,
) -> impl IntoResponse {
    // TODO: List recent observations
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({"error": "not implemented"})))
}

// --- Web Terminal Static Files ---

async fn terminal_index() -> impl IntoResponse {
    let html = include_str!("../../terminal/index.html");
    (
        StatusCode::OK,
        [("Content-Type", "text/html; charset=utf-8")],
        html,
    )
}

async fn terminal_css() -> impl IntoResponse {
    let css = include_str!("../../terminal/style.css");
    (
        StatusCode::OK,
        [("Content-Type", "text/css; charset=utf-8")],
        css,
    )
}

async fn terminal_js() -> impl IntoResponse {
    let js = include_str!("../../terminal/terminal.js");
    (
        StatusCode::OK,
        [("Content-Type", "application/javascript; charset=utf-8")],
        js,
    )
}

async fn terminal_manifest() -> impl IntoResponse {
    let json = include_str!("../../terminal/manifest.json");
    (
        StatusCode::OK,
        [("Content-Type", "application/manifest+json; charset=utf-8")],
        json,
    )
}

async fn terminal_service_worker() -> impl IntoResponse {
    let js = include_str!("../../terminal/service-worker.js");
    (
        StatusCode::OK,
        [("Content-Type", "application/javascript; charset=utf-8")],
        js,
    )
}