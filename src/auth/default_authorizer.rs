//! Default authorizer implementation

use std::sync::Arc;

use crate::auth::authorizer::{AuthResult, AuthorizationAction, Authorizer, RequestContext};
use crate::store::pairing_store::PairingStore;

/// Default authorizer that checks bearer tokens against the pairing store.
pub struct DefaultAuthorizer {
    pairing_store: Arc<dyn PairingStore>,
}

impl DefaultAuthorizer {
    pub fn new(pairing_store: Arc<dyn PairingStore>) -> Self {
        DefaultAuthorizer { pairing_store }
    }
}

impl Authorizer for DefaultAuthorizer {
    fn authenticate_bearer_token(&self, bearer_token: &str) -> AuthResult {
        if bearer_token.is_empty() {
            return AuthResult {
                authenticated: false,
                authorized: false,
                device_id: None,
                reason: "missing bearer token".to_string(),
            };
        }

        for record in self.pairing_store.load_approved_pairings() {
            if record.bearer_token == bearer_token {
                return AuthResult {
                    authenticated: true,
                    authorized: true,
                    device_id: Some(record.device_id),
                    reason: String::new(),
                };
            }
        }

        AuthResult {
            authenticated: false,
            authorized: false,
            device_id: None,
            reason: "invalid bearer token".to_string(),
        }
    }

    fn authorize(&self, context: &RequestContext, action: AuthorizationAction) -> AuthResult {
        match action {
            AuthorizationAction::ApprovePairing | AuthorizationAction::ConfigureHost => {
                if !context.is_local_request {
                    let auth_result = self.authenticate_bearer_token(&context.bearer_token);
                    return AuthResult {
                        authenticated: auth_result.authenticated,
                        authorized: false,
                        device_id: auth_result.device_id,
                        reason: "local request required".to_string(),
                    };
                }
                AuthResult {
                    authenticated: true,
                    authorized: true,
                    device_id: None,
                    reason: String::new(),
                }
            }
            _ => self.authenticate_bearer_token(&context.bearer_token),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::pairing::{DeviceId, DeviceType, PairingRecord};

    struct MockPairingStore {
        approved: Vec<PairingRecord>,
    }

    impl MockPairingStore {
        fn new(approved: Vec<PairingRecord>) -> Self {
            MockPairingStore { approved }
        }
    }

    impl PairingStore for MockPairingStore {
        fn load_pending_pairings(&self) -> Vec<crate::auth::pairing::PairingRequest> {
            vec![]
        }
        fn load_approved_pairings(&self) -> Vec<PairingRecord> {
            self.approved.clone()
        }
        fn upsert_pending_pairing(&self, _: &crate::auth::pairing::PairingRequest) -> bool {
            true
        }
        fn upsert_approved_pairing(&self, _: &PairingRecord) -> bool {
            true
        }
        fn remove_pending_pairing(&self, _: &str) -> bool {
            true
        }
        fn remove_approved_pairing(&self, _: &str) -> bool {
            true
        }
    }

    fn test_record() -> PairingRecord {
        PairingRecord {
            device_id: DeviceId { value: "d_test".to_string() },
            device_name: "Test".to_string(),
            device_type: DeviceType::Desktop,
            bearer_token: "valid_token".to_string(),
            approved_at_unix_ms: 1000,
        }
    }

    #[test]
    fn test_authenticate_valid_token() {
        let authorizer = DefaultAuthorizer::new(Arc::new(MockPairingStore::new(vec![test_record()])));

        let result = authorizer.authenticate_bearer_token("valid_token");
        assert!(result.authenticated);
        assert!(result.authorized);
        assert_eq!(result.device_id.unwrap().value, "d_test");
    }

    #[test]
    fn test_authenticate_empty_token() {
        let authorizer = DefaultAuthorizer::new(Arc::new(MockPairingStore::new(vec![test_record()])));

        let result = authorizer.authenticate_bearer_token("");
        assert!(!result.authenticated);
        assert_eq!(result.reason, "missing bearer token");
    }

    #[test]
    fn test_authenticate_invalid_token() {
        let authorizer = DefaultAuthorizer::new(Arc::new(MockPairingStore::new(vec![test_record()])));

        let result = authorizer.authenticate_bearer_token("wrong_token");
        assert!(!result.authenticated);
        assert_eq!(result.reason, "invalid bearer token");
    }

    #[test]
    fn test_authorize_local_for_sensitive_action() {
        let authorizer = DefaultAuthorizer::new(Arc::new(MockPairingStore::new(vec![])));

        let ctx = RequestContext {
            bearer_token: String::new(),
            client_address: "127.0.0.1".to_string(),
            target: "/pairing".to_string(),
            is_websocket: false,
            is_local_request: true,
        };

        let result = authorizer.authorize(&ctx, AuthorizationAction::ApprovePairing);
        assert!(result.authenticated);
        assert!(result.authorized);
    }

    #[test]
    fn test_authorize_remote_rejected_for_sensitive_action() {
        let authorizer = DefaultAuthorizer::new(Arc::new(MockPairingStore::new(vec![])));

        let ctx = RequestContext {
            bearer_token: "some_token".to_string(),
            client_address: "10.0.0.1".to_string(),
            target: "/pairing".to_string(),
            is_websocket: false,
            is_local_request: false,
        };

        let result = authorizer.authorize(&ctx, AuthorizationAction::ConfigureHost);
        assert!(!result.authorized);
        assert_eq!(result.reason, "local request required");
    }
}