//! Default pairing service implementation

use std::collections::HashMap;
use std::sync::Arc;

use rand::Rng;

use crate::auth::pairing::{
    DeviceId, DeviceType, PairingClaimStatus, PairingRecord, PairingRequest, PairingService,
};
use crate::store::pairing_store::PairingStore;

/// Default pairing TTL in milliseconds (5 minutes).
const DEFAULT_PAIRING_TTL_MS: i64 = 5 * 60 * 1000;

struct ApprovedClaim {
    code: String,
    record: PairingRecord,
}

/// Default implementation of PairingService.
pub struct DefaultPairingService {
    pairing_store: Arc<dyn PairingStore>,
    approved_claims: HashMap<String, ApprovedClaim>,
    rejected_claims: HashMap<String, i64>,
    pairing_request_ttl_ms: i64,
}

impl DefaultPairingService {
    pub fn new(pairing_store: Arc<dyn PairingStore>) -> Self {
        DefaultPairingService {
            pairing_store,
            approved_claims: HashMap::new(),
            rejected_claims: HashMap::new(),
            pairing_request_ttl_ms: DEFAULT_PAIRING_TTL_MS,
        }
    }

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    fn is_expired(&self, request: &PairingRequest, now_ms: i64) -> bool {
        if self.pairing_request_ttl_ms <= 0 {
            return false;
        }
        request.requested_at_unix_ms + self.pairing_request_ttl_ms < now_ms
    }

    fn make_random_hex(bytes: usize) -> String {
        let mut rng = rand::thread_rng();
        (0..bytes).map(|_| format!("{:02x}", rng.gen::<u8>())).collect()
    }

    fn make_pairing_id() -> String {
        format!("p_{}", Self::make_random_hex(8))
    }

    fn make_pairing_code() -> String {
        let mut rng = rand::thread_rng();
        format!("{:06}", rng.gen_range(0..1000000))
    }

    fn make_device_id() -> String {
        format!("d_{}", Self::make_random_hex(8))
    }

    fn make_bearer_token() -> String {
        Self::make_random_hex(24)
    }
}

impl PairingService for DefaultPairingService {
    fn start_pairing(&mut self, device_name: &str, device_type: DeviceType) -> Option<PairingRequest> {
        if device_name.is_empty() {
            return None;
        }

        let now_ms = Self::now_ms();

        // Expire old pending pairings
        for pending in self.pairing_store.load_pending_pairings() {
            if self.is_expired(&pending, now_ms) {
                self.pairing_store.remove_pending_pairing(&pending.pairing_id);
            }
        }

        let request = PairingRequest {
            pairing_id: Self::make_pairing_id(),
            device_name: device_name.to_string(),
            device_type,
            code: Self::make_pairing_code(),
            requested_at_unix_ms: now_ms,
        };

        if request.pairing_id.is_empty() || request.code.len() != 6 {
            return None;
        }

        if !self.pairing_store.upsert_pending_pairing(&request) {
            return None;
        }

        Some(request)
    }

    fn list_pending_pairings(&self) -> Vec<PairingRequest> {
        let now_ms = Self::now_ms();
        let mut pending: Vec<PairingRequest> = self
            .pairing_store
            .load_pending_pairings()
            .into_iter()
            .filter(|p| !self.is_expired(p, now_ms))
            .collect();

        pending.sort_by(|a, b| {
            a.requested_at_unix_ms
                .cmp(&b.requested_at_unix_ms)
                .then_with(|| a.pairing_id.cmp(&b.pairing_id))
        });

        pending
    }

    fn approve_pairing(&mut self, pairing_id: &str, code: &str) -> Option<PairingRecord> {
        let now_ms = Self::now_ms();
        let pending_pairings = self.pairing_store.load_pending_pairings();

        // Expire old pending pairings
        for pending in &pending_pairings {
            if self.is_expired(pending, now_ms) {
                self.pairing_store.remove_pending_pairing(&pending.pairing_id);
            }
        }

        let matched = pending_pairings
            .iter()
            .find(|p| p.pairing_id == pairing_id)?;

        if self.is_expired(matched, now_ms) || matched.code != code {
            return None;
        }

        let record = PairingRecord {
            device_id: DeviceId {
                value: Self::make_device_id(),
            },
            device_name: matched.device_name.clone(),
            device_type: matched.device_type,
            bearer_token: Self::make_bearer_token(),
            approved_at_unix_ms: Self::now_ms(),
        };

        if record.device_id.value.is_empty() || record.bearer_token.is_empty() {
            return None;
        }

        if !self.pairing_store.upsert_approved_pairing(&record) {
            return None;
        }

        if !self.pairing_store.remove_pending_pairing(pairing_id) {
            self.pairing_store.remove_approved_pairing(&record.device_id.value);
            return None;
        }

        self.approved_claims.insert(
            pairing_id.to_string(),
            ApprovedClaim {
                code: code.to_string(),
                record: record.clone(),
            },
        );
        self.rejected_claims.remove(pairing_id);

        Some(record)
    }

    fn claim_approved_pairing(&self, pairing_id: &str, code: &str) -> Option<PairingRecord> {
        let claim = self.approved_claims.get(pairing_id)?;
        if claim.code != code {
            return None;
        }
        Some(claim.record.clone())
    }

    fn get_pairing_claim_status(&self, pairing_id: &str, code: &str) -> PairingClaimStatus {
        if let Some(claim) = self.approved_claims.get(pairing_id) {
            return if claim.code == code {
                PairingClaimStatus::Approved
            } else {
                PairingClaimStatus::Rejected
            };
        }

        if self.rejected_claims.contains_key(pairing_id) {
            return PairingClaimStatus::Rejected;
        }

        let now_ms = Self::now_ms();
        let pending_pairings = self.pairing_store.load_pending_pairings();
        let matched = match pending_pairings.iter().find(|p| p.pairing_id == pairing_id) {
            Some(p) => p,
            None => return PairingClaimStatus::Rejected,
        };

        if self.is_expired(matched, now_ms) {
            return PairingClaimStatus::Expired;
        }

        if matched.code == code {
            PairingClaimStatus::Pending
        } else {
            PairingClaimStatus::Rejected
        }
    }

    fn reject_pairing(&mut self, pairing_id: &str) -> bool {
        self.approved_claims.remove(pairing_id);
        self.rejected_claims.insert(pairing_id.to_string(), Self::now_ms());
        self.pairing_store.remove_pending_pairing(pairing_id)
    }
}