//! Device pairing types and service trait

use serde::{Deserialize, Serialize};

/// Device identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId {
    pub value: String,
}

/// Type of device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum DeviceType {
    #[default]
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "mobile")]
    Mobile,
    #[serde(rename = "desktop")]
    Desktop,
    #[serde(rename = "browser")]
    Browser,
}

impl DeviceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeviceType::Unknown => "unknown",
            DeviceType::Mobile => "mobile",
            DeviceType::Desktop => "desktop",
            DeviceType::Browser => "browser",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "unknown" => Some(DeviceType::Unknown),
            "mobile" => Some(DeviceType::Mobile),
            "desktop" => Some(DeviceType::Desktop),
            "browser" => Some(DeviceType::Browser),
            _ => None,
        }
    }
}

/// A pending pairing request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairingRequest {
    pub pairing_id: String,
    pub device_name: String,
    #[serde(default)]
    pub device_type: DeviceType,
    pub code: String,
    #[serde(default)]
    pub requested_at_unix_ms: i64,
}

/// An approved pairing record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairingRecord {
    pub device_id: DeviceId,
    pub device_name: String,
    #[serde(default)]
    pub device_type: DeviceType,
    pub bearer_token: String,
    #[serde(default)]
    pub approved_at_unix_ms: i64,
}

/// Status of a pairing claim attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PairingClaimStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

/// Trait for pairing service operations.
pub trait PairingService: Send + Sync {
    fn start_pairing(&mut self, device_name: &str, device_type: DeviceType) -> Option<PairingRequest>;
    fn list_pending_pairings(&self) -> Vec<PairingRequest>;
    fn approve_pairing(&mut self, pairing_id: &str, code: &str) -> Option<PairingRecord>;
    fn claim_approved_pairing(&self, pairing_id: &str, code: &str) -> Option<PairingRecord>;
    fn get_pairing_claim_status(&self, pairing_id: &str, code: &str) -> PairingClaimStatus;
    fn reject_pairing(&mut self, pairing_id: &str) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_type_roundtrip() {
        for dt in [DeviceType::Unknown, DeviceType::Mobile, DeviceType::Desktop, DeviceType::Browser] {
            assert_eq!(DeviceType::parse(dt.as_str()), Some(dt));
        }
    }

    #[test]
    fn test_pairing_request_serde_roundtrip() {
        let request = PairingRequest {
            pairing_id: "p_abc".to_string(),
            device_name: "Phone".to_string(),
            device_type: DeviceType::Mobile,
            code: "123456".to_string(),
            requested_at_unix_ms: 1000,
        };
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: PairingRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, deserialized);
    }
}