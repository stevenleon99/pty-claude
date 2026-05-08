//! Shared application state for the HTTP server
//!
//! Holds references to all services needed by route handlers.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::auth::authorizer::Authorizer;
use crate::auth::pairing::PairingService;
use crate::service::observation_store::ObservationStore;
use crate::session::registry::SessionRegistry;
use crate::store::session_store::SessionStore;
use crate::store::host_config::HostConfigStore;

/// Shared application state passed to all axum handlers.
pub struct AppState {
    pub authorizer: Arc<dyn Authorizer>,
    pub pairing_service: Arc<RwLock<dyn PairingService>>,
    pub session_store: Arc<dyn SessionStore>,
    pub host_config_store: Arc<dyn HostConfigStore>,
    pub observation_store: Arc<RwLock<ObservationStore>>,
    /// Active PTY sessions, accessible by WebSocket handlers.
    pub session_registry: Arc<SessionRegistry>,
    /// Password for the web terminal lock screen (from PTY_PASSWORD env, default "1234").
    pub terminal_password: String,
}

// Safety: AppState only contains Arc and RwLock wrapped types, all Send + Sync.
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}
