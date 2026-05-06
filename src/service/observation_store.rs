//! Observation store for tracking events during sessions
//!
//! Stores observation events with limits (max 1000 events).
//! Used for evidence collection and session auditing.

use serde::{Deserialize, Serialize};

/// Maximum number of observation events to store.
const MAX_OBSERVATION_EVENTS: usize = 1000;

/// Kind of observation event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    SessionStarted,
    SessionStopped,
    OutputCaptured,
    FileChanged,
    GitStateChanged,
    ErrorNoted,
    ControlRequested,
    ControlReleased,
    EvidenceCaptured,
}

/// A single observation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationEvent {
    pub event_id: String,
    pub session_id: Option<String>,
    pub kind: ObservationKind,
    pub timestamp_unix_ms: i64,
    pub description: String,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Store for observation events.
pub struct ObservationStore {
    events: Vec<ObservationEvent>,
    max_events: usize,
    next_id: u64,
}

impl Default for ObservationStore {
    fn default() -> Self {
        Self::new(MAX_OBSERVATION_EVENTS)
    }
}

impl ObservationStore {
    pub fn new(max_events: usize) -> Self {
        ObservationStore {
            events: Vec::new(),
            max_events,
            next_id: 1,
        }
    }

    /// Add an observation event.
    pub fn add(
        &mut self,
        session_id: Option<String>,
        kind: ObservationKind,
        timestamp_unix_ms: i64,
        description: String,
        metadata: std::collections::HashMap<String, String>,
    ) -> ObservationEvent {
        let event = ObservationEvent {
            event_id: format!("obs_{:08x}", self.next_id),
            session_id,
            kind,
            timestamp_unix_ms,
            description,
            metadata,
        };
        self.next_id += 1;
        self.events.push(event.clone());

        // Evict oldest if over limit
        while self.events.len() > self.max_events {
            self.events.remove(0);
        }

        event
    }

    /// List events newest first.
    pub fn list_newest_first(&self) -> Vec<&ObservationEvent> {
        self.events.iter().rev().collect()
    }

    /// List events for a specific session.
    pub fn list_for_session(&self, session_id: &str) -> Vec<&ObservationEvent> {
        self.events
            .iter()
            .filter(|e| e.session_id.as_deref() == Some(session_id))
            .collect()
    }

    /// Number of stored events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_list() {
        let mut store = ObservationStore::new(100);
        store.add(
            Some("sess1".to_string()),
            ObservationKind::SessionStarted,
            1000,
            "Session started".to_string(),
            std::collections::HashMap::new(),
        );
        store.add(
            Some("sess1".to_string()),
            ObservationKind::OutputCaptured,
            1001,
            "Output captured".to_string(),
            std::collections::HashMap::new(),
        );

        assert_eq!(store.len(), 2);
        let events = store.list_newest_first();
        assert_eq!(events[0].kind, ObservationKind::OutputCaptured);
        assert_eq!(events[1].kind, ObservationKind::SessionStarted);
    }

    #[test]
    fn test_eviction() {
        let mut store = ObservationStore::new(5);
        for i in 0..10 {
            store.add(
                None,
                ObservationKind::ErrorNoted,
                1000 + i,
                format!("Error {}", i),
                std::collections::HashMap::new(),
            );
        }
        assert_eq!(store.len(), 5);
        // Oldest should be evicted
        let events = store.list_newest_first();
        assert_eq!(events[0].description, "Error 9");
    }

    #[test]
    fn test_list_for_session() {
        let mut store = ObservationStore::new(100);
        store.add(
            Some("sess1".to_string()),
            ObservationKind::SessionStarted,
            1000,
            "S1".to_string(),
            std::collections::HashMap::new(),
        );
        store.add(
            Some("sess2".to_string()),
            ObservationKind::SessionStarted,
            1001,
            "S2".to_string(),
            std::collections::HashMap::new(),
        );

        let events = store.list_for_session("sess1");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].description, "S1");
    }
}
