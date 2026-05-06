//! Networking module - HTTP/WebSocket server, routes, auth middleware
//!
//! Provides:
//! - HTTP REST API for session management, pairing, host config, evidence
//! - WebSocket endpoints for terminal streaming, control, and overview
//! - Bearer token authentication middleware
//! - CORS support
//! - Dual-port server (admin localhost + remote public)

pub mod auth;
pub mod routes;
pub mod server;
pub mod state;
pub mod websocket;

pub use auth::{extract_auth_token, is_local_request, require_auth, build_request_context};
pub use routes::build_api_router;
pub use server::{ServerConfig, run_servers, run_admin_server, run_remote_server, build_full_router};
pub use state::AppState;
