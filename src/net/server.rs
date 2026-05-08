//! HTTP server setup and lifecycle
//!
//! Configures and starts the axum HTTP server with:
//! - Admin listener (localhost only)
//! - Remote listener (all interfaces)
//! - CORS middleware
//! - Auth middleware
//! - Route registration

use axum::Router;
use tokio::net::TcpListener;
use tower_http::cors::{CorsLayer, Any};
use tower_http::trace::TraceLayer;

use crate::store::host_config::HostIdentity;

use super::routes::build_api_router;
use super::state::AppState;

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub admin_host: String,
    pub admin_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub remote_tls: bool,
    pub cors_origins: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            admin_host: "127.0.0.1".to_string(),
            admin_port: 8080,
            remote_host: "0.0.0.0".to_string(),
            remote_port: 8081,
            remote_tls: false,
            cors_origins: vec!["*".to_string()],
        }
    }
}

impl ServerConfig {
    pub fn from_host_identity(identity: &HostIdentity) -> Self {
        ServerConfig {
            admin_host: identity.admin_host.clone(),
            admin_port: identity.admin_port,
            remote_host: identity.remote_host.clone(),
            remote_port: identity.remote_port,
            remote_tls: false,
            cors_origins: vec!["*".to_string()],
        }
    }

    pub fn admin_addr(&self) -> String {
        format!("{}:{}", self.admin_host, self.admin_port)
    }

    pub fn remote_addr(&self) -> String {
        format!("{}:{}", self.remote_host, self.remote_port)
    }
}

/// Build the complete router with API routes and WebSocket routes.
pub fn build_full_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // REST API routes (defined in routes.rs)
        .merge(build_api_router(state))
        // Middleware
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

/// Run the admin server (localhost only).
pub async fn run_admin_server(config: &ServerConfig, state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let addr = config.admin_addr();
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Admin server listening on {}", addr);

    let router = build_full_router(state);

    axum::serve(listener, router).await?;
    Ok(())
}

/// Run the remote server (all interfaces).
pub async fn run_remote_server(config: &ServerConfig, state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let addr = config.remote_addr();
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Remote server listening on {}", addr);

    let router = build_full_router(state);

    // TODO: Add TLS support using rustls if config.remote_tls is true

    axum::serve(listener, router).await?;
    Ok(())
}

/// Run both admin and remote servers concurrently.
pub async fn run_servers(config: ServerConfig, state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let admin_state = state.clone_state();
    let remote_state = state;

    let admin_config = config.clone();
    let remote_config = config;

    let admin_handle = tokio::spawn(async move {
        if let Err(e) = run_admin_server(&admin_config, admin_state).await {
            tracing::error!("Admin server error: {}", e);
        }
    });

    let remote_handle = tokio::spawn(async move {
        if let Err(e) = run_remote_server(&remote_config, remote_state).await {
            tracing::error!("Remote server error: {}", e);
        }
    });

    tokio::select! {
        _ = admin_handle => tracing::warn!("Admin server stopped"),
        _ = remote_handle => tracing::warn!("Remote server stopped"),
    }

    Ok(())
}

/// Clonable state wrapper.
impl Clone for AppState {
    fn clone(&self) -> Self {
        // This is a shallow clone - Arc references are shared
        AppState {
            authorizer: self.authorizer.clone(),
            pairing_service: self.pairing_service.clone(),
            session_store: self.session_store.clone(),
            host_config_store: self.host_config_store.clone(),
            observation_store: self.observation_store.clone(),
            session_registry: self.session_registry.clone(),
            terminal_password: self.terminal_password.clone(),
        }
    }
}

impl AppState {
    pub fn clone_state(&self) -> Self {
        self.clone()
    }
}
