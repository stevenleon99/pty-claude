//! Pairing store trait for device authentication

use crate::auth::pairing::{PairingRecord, PairingRequest};

/// Trait for pairing record persistence.
pub trait PairingStore: Send + Sync {
    fn load_pending_pairings(&self) -> Vec<PairingRequest>;
    fn load_approved_pairings(&self) -> Vec<PairingRecord>;
    fn upsert_pending_pairing(&self, request: &PairingRequest) -> bool;
    fn upsert_approved_pairing(&self, record: &PairingRecord) -> bool;
    fn remove_pending_pairing(&self, pairing_id: &str) -> bool;
    fn remove_approved_pairing(&self, device_id: &str) -> bool;
}