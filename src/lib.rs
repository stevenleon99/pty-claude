//! Sentrits Core - Terminal session management daemon
//!
//! This crate provides PTY-based terminal session management with:
//! - Remote terminal access via HTTP/WebSocket
//! - Device pairing and authentication
//! - Hub integration for relay/remote control
//! - Evidence/log capture and observation storage
//! - Git workspace inspection

pub mod auth;
pub mod cli;
pub mod net;
pub mod service;
pub mod session;
pub mod store;
pub mod util;

pub use session::types::{SessionId, SessionStatus, ProviderType, ControllerKind};
pub use session::snapshot::{SessionSnapshot, SupervisionState, AttentionState};
pub use session::record::SessionRecord;
pub use session::lifecycle::SessionLifecycle;
pub use store::host_config::{HostIdentity, HostConfigStore};
pub use store::session_store::{SessionStore, PersistedSessionRecord};
pub use store::pairing_store::PairingStore;
pub use auth::pairing::{PairingService, PairingRequest, PairingRecord};
pub use auth::authorizer::{Authorizer, AuthResult, RequestContext};
pub use service::{SessionManager, LogBuffer, ObservationStore};
