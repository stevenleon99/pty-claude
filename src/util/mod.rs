//! Utility functions

pub mod debug;

/// Debug tracing support.
pub mod debug_trace {
    use std::sync::Mutex;

    static TRACE_MUTEX: Mutex<()> = Mutex::new(());

    /// Check if debug tracing is enabled via environment variable.
    pub fn is_enabled() -> bool {
        std::env::var("SENTRITS_DEBUG_TRACE").map(|v| !v.is_empty()).unwrap_or(false)
    }

    /// Emit a debug trace message (thread-safe).
    pub fn trace(message: &str) {
        if !is_enabled() {
            return;
        }
        let _lock = TRACE_MUTEX.lock();
        eprintln!("[TRACE] {}", message);
    }
}
