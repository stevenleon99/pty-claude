//! Authentication and device pairing

pub mod authorizer;
pub mod default_authorizer;
pub mod default_pairing_service;
pub mod pairing;

pub use authorizer::{AuthResult, AuthorizationAction, Authorizer, RequestContext};
pub use pairing::{
    DeviceId, DeviceType, PairingClaimStatus, PairingRecord, PairingRequest, PairingService,
};
