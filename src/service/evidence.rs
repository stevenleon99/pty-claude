//! Evidence collection and assembly
//!
//! Provides types for evidence capture requests and results,
//! and the assembler that combines log data with observations.

use serde::{Deserialize, Serialize};

use super::log_buffer::LogEntry;
use super::types::LogStream;

/// Reference to an evidence source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceSourceRef {
    pub session_id: String,
    pub stream: LogStream,
}

/// A single evidence entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceEntry {
    pub entry_id: String,
    pub revision: u64,
    pub byte_start: usize,
    pub byte_end: usize,
    pub timestamp_unix_ms: i64,
    pub stream: LogStream,
    pub text: String,
    pub partial: bool,
}

/// A highlight within evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceHighlight {
    pub start_offset: usize,
    pub end_offset: usize,
    pub label: String,
}

/// Result of evidence capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceResult {
    pub source: EvidenceSourceRef,
    pub entries: Vec<EvidenceEntry>,
    pub highlights: Vec<EvidenceHighlight>,
    pub truncated: bool,
    pub total_bytes: usize,
}

/// Request to assemble evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceAssemblyRequest {
    pub session_id: String,
    pub from_revision: Option<u64>,
    pub to_revision: Option<u64>,
    pub search_pattern: Option<String>,
    pub max_entries: Option<usize>,
}

/// Assemble evidence from log entries.
pub fn assemble_evidence(
    request: &EvidenceAssemblyRequest,
    entries: &[LogEntry],
    total_bytes: usize,
) -> EvidenceResult {
    let max_entries = request.max_entries.unwrap_or(1000);
    let mut evidence_entries = Vec::new();
    let mut truncated = false;

    for (i, entry) in entries.iter().enumerate() {
        // Filter by revision range
        if let Some(from) = request.from_revision {
            if entry.revision < from {
                continue;
            }
        }
        if let Some(to) = request.to_revision {
            if entry.revision > to {
                continue;
            }
        }

        // Filter by search pattern
        if let Some(ref pattern) = request.search_pattern {
            if !entry.text.to_lowercase().contains(&pattern.to_lowercase()) {
                continue;
            }
        }

        if evidence_entries.len() >= max_entries {
            truncated = true;
            break;
        }

        evidence_entries.push(EvidenceEntry {
            entry_id: format!("ev_{:08x}", i),
            revision: entry.revision,
            byte_start: entry.byte_offset,
            byte_end: entry.byte_offset + entry.text.len(),
            timestamp_unix_ms: entry.timestamp_unix_ms,
            stream: entry.stream,
            text: entry.text.clone(),
            partial: false,
        });
    }

    EvidenceResult {
        source: EvidenceSourceRef {
            session_id: request.session_id.clone(),
            stream: LogStream::Stdout, // Default, could be parameterized
        },
        entries: evidence_entries,
        highlights: vec![],
        truncated,
        total_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::log_buffer::LogBuffer;

    #[test]
    fn test_assemble_evidence_basic() {
        let mut buf = LogBuffer::new(1024, 100);
        buf.append_stdout("line1\nline2\nline3\n", 1000);

        let request = EvidenceAssemblyRequest {
            session_id: "sess1".to_string(),
            from_revision: None,
            to_revision: None,
            search_pattern: None,
            max_entries: None,
        };

        let result = assemble_evidence(&request, buf.entries(), buf.total_bytes());
        assert_eq!(result.entries.len(), 3);
        assert!(!result.truncated);
    }

    #[test]
    fn test_assemble_evidence_with_search() {
        let mut buf = LogBuffer::new(1024, 100);
        buf.append_stdout("error: bad\ninfo: ok\nerror: worse\n", 1000);

        let request = EvidenceAssemblyRequest {
            session_id: "sess1".to_string(),
            from_revision: None,
            to_revision: None,
            search_pattern: Some("error".to_string()),
            max_entries: None,
        };

        let result = assemble_evidence(&request, buf.entries(), buf.total_bytes());
        assert_eq!(result.entries.len(), 2);
    }

    #[test]
    fn test_assemble_evidence_truncated() {
        let mut buf = LogBuffer::new(1024, 100);
        for i in 0..20 {
            buf.append_stdout(&format!("line{}\n", i), 1000 + i);
        }

        let request = EvidenceAssemblyRequest {
            session_id: "sess1".to_string(),
            from_revision: None,
            to_revision: None,
            search_pattern: None,
            max_entries: Some(5),
        };

        let result = assemble_evidence(&request, buf.entries(), buf.total_bytes());
        assert_eq!(result.entries.len(), 5);
        assert!(result.truncated);
    }
}
