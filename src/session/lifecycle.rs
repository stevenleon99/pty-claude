//! Session lifecycle state machine

use crate::session::types::SessionStatus;

/// Check if a transition from one status to another is valid.
pub fn is_valid_transition(from: SessionStatus, to: SessionStatus) -> bool {
    match from {
        SessionStatus::Created => to == SessionStatus::Starting,
        SessionStatus::Starting => {
            to == SessionStatus::Running || to == SessionStatus::Error
        }
        SessionStatus::Running => {
            to == SessionStatus::AwaitingInput
                || to == SessionStatus::Exited
                || to == SessionStatus::Error
        }
        SessionStatus::AwaitingInput => {
            to == SessionStatus::Running
                || to == SessionStatus::Exited
                || to == SessionStatus::Error
        }
        SessionStatus::Exited | SessionStatus::Error => false,
    }
}

/// Session lifecycle state machine.
#[derive(Debug, Clone)]
pub struct SessionLifecycle {
    state: SessionStatus,
}

impl SessionLifecycle {
    pub fn new() -> Self {
        SessionLifecycle {
            state: SessionStatus::Created,
        }
    }

    pub fn state(&self) -> SessionStatus {
        self.state
    }

    /// Attempt to transition to a new state. Returns true if successful.
    pub fn try_transition(&mut self, next_state: SessionStatus) -> bool {
        if !is_valid_transition(self.state, next_state) {
            return false;
        }
        self.state = next_state;
        true
    }
}

impl Default for SessionLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        assert!(is_valid_transition(SessionStatus::Created, SessionStatus::Starting));
        assert!(is_valid_transition(SessionStatus::Starting, SessionStatus::Running));
        assert!(is_valid_transition(SessionStatus::Starting, SessionStatus::Error));
        assert!(is_valid_transition(SessionStatus::Running, SessionStatus::AwaitingInput));
        assert!(is_valid_transition(SessionStatus::Running, SessionStatus::Exited));
        assert!(is_valid_transition(SessionStatus::Running, SessionStatus::Error));
        assert!(is_valid_transition(SessionStatus::AwaitingInput, SessionStatus::Running));
        assert!(is_valid_transition(SessionStatus::AwaitingInput, SessionStatus::Exited));
        assert!(is_valid_transition(SessionStatus::AwaitingInput, SessionStatus::Error));
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(!is_valid_transition(SessionStatus::Created, SessionStatus::Running));
        assert!(!is_valid_transition(SessionStatus::Created, SessionStatus::Exited));
        assert!(!is_valid_transition(SessionStatus::Exited, SessionStatus::Running));
        assert!(!is_valid_transition(SessionStatus::Error, SessionStatus::Running));
        assert!(!is_valid_transition(SessionStatus::Running, SessionStatus::Starting));
    }

    #[test]
    fn test_lifecycle_full_flow() {
        let mut lifecycle = SessionLifecycle::new();
        assert_eq!(lifecycle.state(), SessionStatus::Created);

        assert!(lifecycle.try_transition(SessionStatus::Starting));
        assert_eq!(lifecycle.state(), SessionStatus::Starting);

        assert!(lifecycle.try_transition(SessionStatus::Running));
        assert_eq!(lifecycle.state(), SessionStatus::Running);

        assert!(lifecycle.try_transition(SessionStatus::AwaitingInput));
        assert_eq!(lifecycle.state(), SessionStatus::AwaitingInput);

        assert!(lifecycle.try_transition(SessionStatus::Running));
        assert!(lifecycle.try_transition(SessionStatus::Exited));
        assert_eq!(lifecycle.state(), SessionStatus::Exited);
    }

    #[test]
    fn test_lifecycle_rejects_invalid() {
        let mut lifecycle = SessionLifecycle::new();
        assert!(!lifecycle.try_transition(SessionStatus::Running));
        assert_eq!(lifecycle.state(), SessionStatus::Created);
    }
}
