//! Host configuration store trait and types

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::session::types::ProviderType;

/// Default maximum number of launch records to retain.
pub const DEFAULT_MAX_LAUNCH_RECORDS: usize = 50;

/// Default admin host address.
pub const DEFAULT_ADMIN_HOST: &str = "127.0.0.1";

/// Default admin port.
pub const DEFAULT_ADMIN_PORT: u16 = 18085;

/// Default remote host address.
pub const DEFAULT_REMOTE_HOST: &str = "0.0.0.0";

/// Default remote port.
pub const DEFAULT_REMOTE_PORT: u16 = 18086;

/// Default display name.
pub const DEFAULT_DISPLAY_NAME: &str = "Sentrits Host";

/// Provider command override configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderCommandOverride {
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// A bounded recent-launch record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchRecord {
    pub record_id: String,
    pub provider: ProviderType,
    pub workspace_root: String,
    pub title: String,
    pub launched_at_unix_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub group_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_argv: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_shell: Option<String>,
}

/// Complete host identity and configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostIdentity {
    #[serde(default)]
    pub host_id: String,
    #[serde(default = "default_display_name")]
    pub display_name: String,
    #[serde(default)]
    pub certificate_pem_path: String,
    #[serde(default)]
    pub private_key_pem_path: String,
    #[serde(default = "default_admin_host")]
    pub admin_host: String,
    #[serde(default = "default_admin_port")]
    pub admin_port: u16,
    #[serde(default = "default_remote_host")]
    pub remote_host: String,
    #[serde(default = "default_remote_port")]
    pub remote_port: u16,
    #[serde(default)]
    pub codex_command: Option<ProviderCommandOverride>,
    #[serde(default)]
    pub claude_command: Option<ProviderCommandOverride>,
    #[serde(default)]
    pub launch_records: Vec<LaunchRecord>,
    #[serde(default = "default_max_launch_records")]
    pub max_launch_records: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap_shell_path: Option<String>,
    #[serde(default)]
    pub import_service_manager_environment: bool,
    #[serde(default)]
    pub service_manager_environment_allowlist: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hub_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hub_token: Option<String>,
}

fn default_display_name() -> String {
    DEFAULT_DISPLAY_NAME.to_string()
}
fn default_admin_host() -> String {
    DEFAULT_ADMIN_HOST.to_string()
}
fn default_admin_port() -> u16 {
    DEFAULT_ADMIN_PORT
}
fn default_remote_host() -> String {
    DEFAULT_REMOTE_HOST.to_string()
}
fn default_remote_port() -> u16 {
    DEFAULT_REMOTE_PORT
}
fn default_max_launch_records() -> usize {
    DEFAULT_MAX_LAUNCH_RECORDS
}

impl Default for HostIdentity {
    fn default() -> Self {
        HostIdentity {
            host_id: String::new(),
            display_name: DEFAULT_DISPLAY_NAME.to_string(),
            certificate_pem_path: String::new(),
            private_key_pem_path: String::new(),
            admin_host: DEFAULT_ADMIN_HOST.to_string(),
            admin_port: DEFAULT_ADMIN_PORT,
            remote_host: DEFAULT_REMOTE_HOST.to_string(),
            remote_port: DEFAULT_REMOTE_PORT,
            codex_command: None,
            claude_command: None,
            launch_records: Vec::new(),
            max_launch_records: DEFAULT_MAX_LAUNCH_RECORDS,
            bootstrap_shell_path: None,
            import_service_manager_environment: false,
            service_manager_environment_allowlist: Vec::new(),
            hub_url: None,
            hub_token: None,
        }
    }
}

/// Trait for host configuration persistence.
pub trait HostConfigStore: Send + Sync {
    fn load_host_identity(&self) -> Option<HostIdentity>;
    fn save_host_identity(&self, identity: &HostIdentity) -> bool;
    fn storage_root(&self) -> &PathBuf;
}

/// Ensure a host identity exists, generating a new host ID if needed.
pub fn ensure_host_identity(store: &mut dyn HostConfigStore) -> Option<HostIdentity> {
    let mut identity = store.load_host_identity().unwrap_or_default();
    if !identity.host_id.is_empty() {
        return Some(identity);
    }

    if identity.display_name.is_empty() {
        identity.display_name = DEFAULT_DISPLAY_NAME.to_string();
    }
    identity.host_id = generate_host_id();
    if !store.save_host_identity(&identity) {
        return None;
    }
    Some(identity)
}

fn generate_host_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    format!(
        "h_{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}",
        hex_digit(bytes[0] >> 4),
        hex_digit(bytes[0] & 0xf),
        hex_digit(bytes[1] >> 4),
        hex_digit(bytes[1] & 0xf),
        hex_digit(bytes[2] >> 4),
        hex_digit(bytes[2] & 0xf),
        hex_digit(bytes[3] >> 4),
        hex_digit(bytes[3] & 0xf),
        hex_digit(bytes[4] >> 4),
        hex_digit(bytes[4] & 0xf),
        hex_digit(bytes[5] >> 4),
        hex_digit(bytes[5] & 0xf),
        hex_digit(bytes[6] >> 4),
        hex_digit(bytes[6] & 0xf),
        hex_digit(bytes[7] >> 4),
        hex_digit(bytes[7] & 0xf),
        hex_digit(bytes[8] >> 4),
        hex_digit(bytes[8] & 0xf),
        hex_digit(bytes[9] >> 4),
        hex_digit(bytes[9] & 0xf),
        hex_digit(bytes[10] >> 4),
        hex_digit(bytes[10] & 0xf),
        hex_digit(bytes[11] >> 4),
        hex_digit(bytes[11] & 0xf),
        hex_digit(bytes[12] >> 4),
        hex_digit(bytes[12] & 0xf),
        hex_digit(bytes[13] >> 4),
        hex_digit(bytes[13] & 0xf),
        hex_digit(bytes[14] >> 4),
        hex_digit(bytes[14] & 0xf),
        hex_digit(bytes[15] >> 4),
        hex_digit(bytes[15] & 0xf),
    )
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + nibble - 10) as char,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_host_identity() {
        let identity = HostIdentity::default();
        assert!(identity.host_id.is_empty());
        assert_eq!(identity.display_name, DEFAULT_DISPLAY_NAME);
        assert_eq!(identity.admin_host, DEFAULT_ADMIN_HOST);
        assert_eq!(identity.admin_port, DEFAULT_ADMIN_PORT);
        assert_eq!(identity.remote_host, DEFAULT_REMOTE_HOST);
        assert_eq!(identity.remote_port, DEFAULT_REMOTE_PORT);
    }

    #[test]
    fn test_generate_host_id() {
        let id = generate_host_id();
        assert!(id.starts_with("h_"));
        assert_eq!(id.len(), 2 + 32); // "h_" + 32 hex chars
    }

    #[test]
    fn test_host_identity_serde_roundtrip() {
 let identity = HostIdentity::default();
        let json = serde_json::to_string(&identity).unwrap();
        let deserialized: HostIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(identity, deserialized);
    }
}