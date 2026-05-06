//! Authorizer trait and types

use serde::{Deserialize, Serialize};

use crate::auth::pairing::DeviceId;

/// Actions that can be authorized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthorizationAction {
    ObserveSessions,
    ControlSession,
    ApprovePairing,
    ConfigureHost,
    ManageHostSetups,
}

/// Context for an authorization request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestContext {
    pub bearer_token: String,
    pub client_address: String,
    pub target: String,
    #[serde(default)]
    pub is_websocket: bool,
    #[serde(default)]
    pub is_local_request: bool,
}

/// Result of authentication and authorization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthResult {
    #[serde(default)]
    pub authenticated: bool,
    #[serde(default)]
    pub authorized: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<DeviceId>,
    #[serde(default)]
    pub reason: String,
}

/// Trait for authentication and authorization.
pub trait Authorizer: Send + Sync {
    fn authenticate_bearer_token(&self, bearer_token: &str) -> AuthResult;
    fn authorize(&self, context: &RequestContext, action: AuthorizationAction) -> AuthResult;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_context_serde_roundtrip() {
        let ctx = RequestContext {
            bearer_token: "token123".to_string(),
            client_address: "127.0.0.1".to_string(),
            target: "/sessions".to_string(),
            is_websocket: true,
            is_local_request: true,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let deserialized: RequestContext = serde_json::from_str(&json).unwrap();
        assert_eq!(ctx, deserialized);
    }
}