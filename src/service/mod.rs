//! Service layer - session management, evidence collection, observations
//!
//! This module provides:
//! - SessionManager: Central service for session lifecycle
//! - LogBuffer: Evidence collection for managed log sessions
//! - ObservationStore: Event tracking for auditing
//! - Evidence assembly: Combining logs with observations

pub mod evidence;
pub mod log_buffer;
pub mod observation_store;
pub mod session_manager;
pub mod types;

pub use evidence::{assemble_evidence, EvidenceAssemblyRequest, EvidenceEntry, EvidenceResult};
pub use log_buffer::LogBuffer;
pub use observation_store::{ObservationEvent, ObservationKind, ObservationStore};
pub use session_manager::{SessionEntry, SessionManager};
pub use types::*;
