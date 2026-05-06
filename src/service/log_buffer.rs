//! Log buffer for evidence collection
//!
//! Stores stdout/stderr output from managed log sessions with
//! revision tracking, search, and size limits.

use serde::{Deserialize, Serialize};

use super::types::LogStream;

/// Maximum log buffer size in bytes (16 MB).
const MAX_LOG_BUFFER_BYTES: usize = 16 * 1024 * 1024;
/// Maximum number of log entries.
const MAX_LOG_ENTRIES: usize = 50_000;

/// A single log entry in the buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub revision: u64,
    pub stream: LogStream,
    pub timestamp_unix_ms: i64,
    pub text: String,
    pub byte_offset: usize,
}

/// Range query result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRange {
    pub entries: Vec<LogEntry>,
    pub total_bytes: usize,
}

/// Search result with context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSearchResult {
    pub matches: Vec<LogSearchMatch>,
    pub total_matches: usize,
    pub truncated: bool,
}

/// A single search match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSearchMatch {
    pub revision: u64,
    pub stream: LogStream,
    pub line_number: usize,
    pub text: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

/// Log buffer with capacity limits and revision tracking.
pub struct LogBuffer {
    entries: Vec<LogEntry>,
    total_bytes: usize,
    next_revision: u64,
    max_bytes: usize,
    max_entries: usize,
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new(MAX_LOG_BUFFER_BYTES, MAX_LOG_ENTRIES)
    }
}

impl LogBuffer {
    pub fn new(max_bytes: usize, max_entries: usize) -> Self {
        LogBuffer {
            entries: Vec::new(),
            total_bytes: 0,
            next_revision: 1,
            max_bytes,
            max_entries,
        }
    }

    /// Append stdout data, splitting by lines.
    pub fn append_stdout(&mut self, text: &str, timestamp_ms: i64) {
        self.append(text, LogStream::Stdout, timestamp_ms);
    }

    /// Append stderr data, splitting by lines.
    pub fn append_stderr(&mut self, text: &str, timestamp_ms: i64) {
        self.append(text, LogStream::Stderr, timestamp_ms);
    }

    fn append(&mut self, text: &str, stream: LogStream, timestamp_ms: i64) {
        for line in text.lines() {
            if line.is_empty() {
                continue;
            }

            let byte_offset = self.total_bytes;
            let line_bytes = line.len();

            let entry = LogEntry {
                revision: self.next_revision,
                stream,
                timestamp_unix_ms: timestamp_ms,
                text: line.to_string(),
                byte_offset,
            };

            self.next_revision += 1;
            self.total_bytes += line_bytes;

            self.entries.push(entry);
            self.evict_if_needed();
        }
    }

    /// Get recent entries (tail).
    pub fn tail(&self, count: usize) -> Vec<&LogEntry> {
        let start = self.entries.len().saturating_sub(count);
        self.entries[start..].iter().collect()
    }

    /// Get entries within a revision range.
    pub fn range(&self, from_revision: u64, to_revision: Option<u64>) -> LogRange {
        let entries: Vec<&LogEntry> = self
            .entries
            .iter()
            .filter(|e| {
                if e.revision < from_revision {
                    return false;
                }
                if let Some(to) = to_revision {
                    if e.revision > to {
                        return false;
                    }
                }
                true
            })
            .collect();

        LogRange {
            entries: entries.into_iter().cloned().collect(),
            total_bytes: self.total_bytes,
        }
    }

    /// Search entries for a pattern (case-insensitive substring).
    pub fn search(&self, pattern: &str, max_results: usize) -> LogSearchResult {
        let pattern_lower = pattern.to_lowercase();
        let mut matches = Vec::new();

        for (i, entry) in self.entries.iter().enumerate() {
            if entry.text.to_lowercase().contains(&pattern_lower) {
                let context_before = self
                    .entries
                    .iter()
                    .take(i)
                    .rev()
                    .take(3)
                    .map(|e| e.text.clone())
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();

                let context_after = self
                    .entries
                    .iter()
                    .skip(i + 1)
                    .take(3)
                    .map(|e| e.text.clone())
                    .collect();

                matches.push(LogSearchMatch {
                    revision: entry.revision,
                    stream: entry.stream,
                    line_number: i,
                    text: entry.text.clone(),
                    context_before,
                    context_after,
                });

                if matches.len() >= max_results {
                    break;
                }
            }
        }

        let truncated = matches.len() >= max_results
            && self.entries.iter().any(|e| {
                e.text.to_lowercase().contains(&pattern_lower)
                    && !matches.iter().any(|m| m.revision == e.revision)
            });

        LogSearchResult {
            total_matches: matches.len(),
            matches,
            truncated,
        }
    }

    /// Get context around a specific revision.
    pub fn context(&self, revision: u64, before: usize, after: usize) -> Vec<&LogEntry> {
        let idx = match self.entries.iter().position(|e| e.revision == revision) {
            Some(i) => i,
            None => return vec![],
        };

        let start = idx.saturating_sub(before);
        let end = (idx + after + 1).min(self.entries.len());
        self.entries[start..end].iter().collect()
    }

    /// Current revision (next to be assigned).
    pub fn current_revision(&self) -> u64 {
        self.next_revision
    }

    /// Total bytes stored.
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get a reference to all entries.
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    fn evict_if_needed(&mut self) {
        while self.entries.len() > self.max_entries || self.total_bytes > self.max_bytes {
            if let Some(removed) = self.entries.first() {
                self.total_bytes = self.total_bytes.saturating_sub(removed.text.len());
            }
            self.entries.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_and_tail() {
        let mut buf = LogBuffer::new(1024, 100);
        buf.append_stdout("line 1\nline 2\nline 3\n", 1000);

        assert_eq!(buf.len(), 3);
        let tail = buf.tail(2);
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].text, "line 2");
        assert_eq!(tail[1].text, "line 3");
    }

    #[test]
    fn test_range_query() {
        let mut buf = LogBuffer::new(1024, 100);
        buf.append_stdout("a\nb\nc\n", 1000);

        let range = buf.range(2, Some(3));
        assert_eq!(range.entries.len(), 2);
        assert_eq!(range.entries[0].text, "b");
        assert_eq!(range.entries[1].text, "c");
    }

    #[test]
    fn test_search() {
        let mut buf = LogBuffer::new(1024, 100);
        buf.append_stdout("error: something failed\ninfo: ok\nerror: again\n", 1000);

        let result = buf.search("error", 10);
        assert_eq!(result.matches.len(), 2);
        assert_eq!(result.matches[0].text, "error: something failed");
    }

    #[test]
    fn test_eviction() {
        let mut buf = LogBuffer::new(20, 5);
        for i in 0..10 {
            buf.append_stdout(&format!("line{}\n", i), 1000 + i);
        }

        assert!(buf.len() <= 5);
        assert!(buf.total_bytes() <= 20);
    }

    #[test]
    fn test_context() {
        let mut buf = LogBuffer::new(1024, 100);
        buf.append_stdout("a\nb\nc\nd\ne\n", 1000);

        let ctx = buf.context(3, 1, 1);
        assert_eq!(ctx.len(), 3);
        assert_eq!(ctx[0].text, "b");
        assert_eq!(ctx[1].text, "c");
        assert_eq!(ctx[2].text, "d");
    }

    #[test]
    fn test_mixed_streams() {
        let mut buf = LogBuffer::new(1024, 100);
        buf.append_stdout("out\n", 1000);
        buf.append_stderr("err\n", 1001);

        assert_eq!(buf.len(), 2);
        assert_eq!(buf.entries[0].stream, LogStream::Stdout);
        assert_eq!(buf.entries[1].stream, LogStream::Stderr);
    }
}
