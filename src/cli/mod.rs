//! CLI daemon client (Phase 7)

pub struct DaemonEndpoint {
    pub host: String,
    pub port: u16,
}

impl Default for DaemonEndpoint {
    fn default() -> Self {
        DaemonEndpoint {
            host: "127.0.0.1".to_string(),
            port: 18085,
        }
    }
}
