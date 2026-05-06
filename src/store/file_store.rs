//! File-based store implementations

use std::fs;
use std::path::{Path, PathBuf};

use crate::auth::pairing::{PairingRecord, PairingRequest};
use crate::store::host_config::{HostConfigStore, HostIdentity};
use crate::store::pairing_store::PairingStore;
use crate::store::session_store::{PersistedSessionRecord, SessionStore};

/// File-based host configuration store.
pub struct FileHostConfigStore {
    storage_root: PathBuf,
}

impl FileHostConfigStore {
    pub fn new(storage_root: PathBuf) -> Self {
        FileHostConfigStore { storage_root }
    }

    fn host_identity_path(&self) -> PathBuf {
        self.storage_root.join("host_identity.json")
    }
}

impl HostConfigStore for FileHostConfigStore {
    fn load_host_identity(&self) -> Option<HostIdentity> {
        let path = self.host_identity_path();
        let content = read_file(&path)?;
        serde_json::from_str(&content).ok()
    }

    fn save_host_identity(&self, identity: &HostIdentity) -> bool {
        let path = self.host_identity_path();
        let json = match serde_json::to_string_pretty(identity) {
            Ok(j) => j,
            Err(_) => return false,
        };
        write_file_atomically(&path, &json)
    }

    fn storage_root(&self) -> &PathBuf {
        &self.storage_root
    }
}

/// File-based pairing store.
pub struct FilePairingStore {
    storage_root: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct PairingSnapshot {
    #[serde(default)]
    pending: Vec<PairingRequest>,
    #[serde(default)]
    approved: Vec<PairingRecord>,
}

impl FilePairingStore {
    pub fn new(storage_root: PathBuf) -> Self {
        FilePairingStore { storage_root }
    }

    fn pairings_path(&self) -> PathBuf {
        self.storage_root.join("pairings.json")
    }

    fn load_snapshot(&self) -> PairingSnapshot {
        let content = read_file(&self.pairings_path()).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_default()
    }

    fn save_snapshot(&self, snapshot: &PairingSnapshot) -> bool {
        let json = match serde_json::to_string_pretty(snapshot) {
            Ok(j) => j,
            Err(_) => return false,
        };
        write_file_atomically(&self.pairings_path(), &json)
    }
}

impl PairingStore for FilePairingStore {
    fn load_pending_pairings(&self) -> Vec<PairingRequest> {
        self.load_snapshot().pending
    }

    fn load_approved_pairings(&self) -> Vec<PairingRecord> {
        self.load_snapshot().approved
    }

    fn upsert_pending_pairing(&self, request: &PairingRequest) -> bool {
        let mut snapshot = self.load_snapshot();
        if let Some(existing) = snapshot
            .pending
            .iter_mut()
            .find(|p| p.pairing_id == request.pairing_id)
        {
            *existing = request.clone();
        } else {
            snapshot.pending.push(request.clone());
        }
        self.save_snapshot(&snapshot)
    }

    fn upsert_approved_pairing(&self, record: &PairingRecord) -> bool {
        let mut snapshot = self.load_snapshot();
        if let Some(existing) = snapshot
            .approved
            .iter_mut()
            .find(|p| p.device_id.value == record.device_id.value)
        {
            *existing = record.clone();
        } else {
            snapshot.approved.push(record.clone());
        }
        self.save_snapshot(&snapshot)
    }

    fn remove_pending_pairing(&self, pairing_id: &str) -> bool {
        let mut snapshot = self.load_snapshot();
        let before = snapshot.pending.len();
        snapshot
            .pending
            .retain(|p| p.pairing_id != pairing_id);
        if snapshot.pending.len() == before {
            return false;
        }
        self.save_snapshot(&snapshot)
    }

    fn remove_approved_pairing(&self, device_id: &str) -> bool {
        let mut snapshot = self.load_snapshot();
        let before = snapshot.approved.len();
        snapshot
            .approved
            .retain(|r| r.device_id.value != device_id);
        if snapshot.approved.len() == before {
            return false;
        }
        self.save_snapshot(&snapshot)
    }
}

/// File-based session store.
pub struct FileSessionStore {
    storage_root: PathBuf,
}

impl FileSessionStore {
    pub fn new(storage_root: PathBuf) -> Self {
        FileSessionStore { storage_root }
    }

    fn sessions_path(&self) -> PathBuf {
        self.storage_root.join("sessions.json")
    }

    fn load_records(&self) -> Vec<PersistedSessionRecord> {
        let content = read_file(&self.sessions_path()).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_default()
    }
}

impl SessionStore for FileSessionStore {
    fn load_sessions(&self) -> Vec<PersistedSessionRecord> {
        self.load_records()
    }

    fn upsert_session_record(&self, record: &PersistedSessionRecord) -> bool {
        let mut records = self.load_records();
        if let Some(existing) = records
            .iter_mut()
            .find(|r| r.session_id == record.session_id)
        {
            *existing = record.clone();
        } else {
            records.push(record.clone());
        }
        let json = match serde_json::to_string_pretty(&records) {
            Ok(j) => j,
            Err(_) => return false,
        };
        write_file_atomically(&self.sessions_path(), &json)
    }

    fn remove_session_record(&self, session_id: &str) -> bool {
        let mut records = self.load_records();
        let before = records.len();
        records.retain(|r| r.session_id != session_id);
        if records.len() == before {
            return false;
        }
        let json = match serde_json::to_string_pretty(&records) {
            Ok(j) => j,
            Err(_) => return false,
        };
        write_file_atomically(&self.sessions_path(), &json)
    }
}

// --- File I/O helpers ---

fn read_file(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn write_file_atomically(path: &Path, data: &str) -> bool {
    if let Some(parent) = path.parent() {
        if let Err(_) = fs::create_dir_all(parent) {
            return false;
        }
    }

    let temp_path = path.with_extension("tmp");
    if let Err(_) = fs::write(&temp_path, data) {
        return false;
    }

    if let Err(_) = fs::rename(&temp_path, path) {
        // Try remove + rename as fallback (cross-device, etc.)
        let _ = fs::remove_file(path);
        if let Err(_) = fs::rename(&temp_path, path) {
            let _ = fs::remove_file(&temp_path);
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::pairing::{DeviceType};
    use crate::session::types::{ProviderType, SessionStatus};

    fn temp_dir() -> PathBuf {
        tempfile::tempdir().unwrap().keep()
    }

    #[test]
    fn test_file_host_config_store_roundtrip() {
        let dir = temp_dir();
        let store = FileHostConfigStore::new(dir.clone());

        assert!(store.load_host_identity().is_none());

        let identity = HostIdentity::default();
        assert!(store.save_host_identity(&identity));

        let loaded = store.load_host_identity().unwrap();
        assert_eq!(loaded.display_name, "Sentrits Host");
    }

    #[test]
    fn test_file_pairing_store_roundtrip() {
        let dir = temp_dir();
        let store = FilePairingStore::new(dir);

        let request = PairingRequest {
            pairing_id: "p_test".to_string(),
            device_name: "Phone".to_string(),
            device_type: DeviceType::Mobile,
            code: "123456".to_string(),
            requested_at_unix_ms: 1000,
        };

        assert!(store.upsert_pending_pairing(&request));
        let pending = store.load_pending_pairings();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].pairing_id, "p_test");

        assert!(store.remove_pending_pairing("p_test"));
        assert!(store.load_pending_pairings().is_empty());
        assert!(!store.remove_pending_pairing("p_test")); // already removed
    }

    #[test]
    fn test_file_session_store_roundtrip() {
        let dir = temp_dir();
        let store = FileSessionStore::new(dir);

        let record = PersistedSessionRecord {
            session_id: "s_001".to_string(),
            provider: ProviderType::Codex,
            workspace_root: "/tmp".to_string(),
            title: "Test".to_string(),
            status: SessionStatus::Running,
            conversation_id: None,
            group_tags: vec![],
            current_sequence: 10,
            recent_terminal_tail: "output".to_string(),
        };

        assert!(store.upsert_session_record(&record));
        let sessions = store.load_sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "s_001");

        assert!(store.remove_session_record("s_001"));
        assert!(store.load_sessions().is_empty());
    }

    #[test]
    fn test_file_session_store_upsert_updates() {
        let dir = temp_dir();
        let store = FileSessionStore::new(dir);

        let mut record = PersistedSessionRecord {
            session_id: "s_001".to_string(),
            provider: ProviderType::Codex,
            workspace_root: "/tmp".to_string(),
            title: "Test".to_string(),
            status: SessionStatus::Running,
            conversation_id: None,
            group_tags: vec![],
            current_sequence: 10,
            recent_terminal_tail: "output".to_string(),
        };

        store.upsert_session_record(&record);

        record.status = SessionStatus::Exited;
        record.current_sequence = 99;
        store.upsert_session_record(&record);

        let sessions = store.load_sessions();
        assert_eq!(sessions.len(), 1); // upsert, not append
        assert_eq!(sessions[0].status, SessionStatus::Exited);
        assert_eq!(sessions[0].current_sequence, 99);
    }
}