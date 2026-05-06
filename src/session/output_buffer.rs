//! Session output buffer with sequence tracking

use std::collections::VecDeque;

/// A single chunk of output data.
#[derive(Debug, Clone)]
struct OutputChunk {
    seq: u64,
    data: String,
}

/// A slice of output data spanning a range of sequences.
#[derive(Debug, Clone, Default)]
pub struct OutputSlice {
    pub seq_start: u64,
    pub seq_end: u64,
    pub data: String,
}

/// Ring buffer for session output with capacity limits and sequence tracking.
#[derive(Debug)]
pub struct SessionOutputBuffer {
    capacity_bytes: usize,
    size_bytes: usize,
    next_sequence: u64,
    chunks: VecDeque<OutputChunk>,
}

impl SessionOutputBuffer {
    pub fn new(capacity_bytes: usize) -> Self {
        SessionOutputBuffer {
            capacity_bytes,
            size_bytes: 0,
            next_sequence: 0,
            chunks: VecDeque::new(),
        }
    }

    /// Append output data to the buffer.
    pub fn append(&mut self, data: String) {
        if data.is_empty() {
            return;
        }

        let mut data = data;
        if data.len() > self.capacity_bytes {
            data = data[data.len() - self.capacity_bytes..].to_string();
        }

        self.size_bytes += data.len();
        self.chunks.push_back(OutputChunk {
            seq: self.next_sequence,
            data,
        });
        self.next_sequence += 1;

        self.evict_if_needed();
    }

    pub fn capacity_bytes(&self) -> usize {
        self.capacity_bytes
    }

    pub fn size_bytes(&self) -> usize {
        self.size_bytes
    }

    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Get the sequence number of the latest chunk.
    pub fn latest_sequence(&self) -> Option<u64> {
        self.chunks.back().map(|c| c.seq)
    }

    /// Get the last `max_bytes` of output.
    pub fn tail(&self, max_bytes: usize) -> OutputSlice {
        if self.chunks.is_empty() || max_bytes == 0 {
            return OutputSlice::default();
        }

        let mut bytes_collected = 0usize;
        let mut first_index = self.chunks.len();

        for i in (0..self.chunks.len()).rev() {
            if bytes_collected >= max_bytes {
                break;
            }
            let chunk = &self.chunks[i];
            bytes_collected += chunk.data.len().min(max_bytes);
            first_index = i;
        }

        let mut slice = OutputSlice {
            seq_start: self.chunks[first_index].seq,
            seq_end: self.chunks.back().unwrap().seq,
            data: String::new(),
        };

        for i in first_index..self.chunks.len() {
            slice.data.push_str(&self.chunks[i].data);
        }

        if slice.data.len() > max_bytes {
            slice.data = slice.data[slice.data.len() - max_bytes..].to_string();
        }

        slice
    }

    /// Get all output starting from a given sequence number.
    pub fn slice_from_sequence(&self, first_sequence: u64) -> OutputSlice {
        if self.chunks.is_empty() {
            return OutputSlice::default();
        }

        let first_index = match self.chunks.iter().position(|c| c.seq >= first_sequence) {
            Some(i) => i,
            None => return OutputSlice::default(),
        };

        let mut slice = OutputSlice {
            seq_start: self.chunks[first_index].seq,
            seq_end: self.chunks.back().unwrap().seq,
            data: String::new(),
        };

        for i in first_index..self.chunks.len() {
            slice.data.push_str(&self.chunks[i].data);
        }

        slice
    }

    fn evict_if_needed(&mut self) {
        while self.size_bytes > self.capacity_bytes && !self.chunks.is_empty() {
            if let Some(front) = self.chunks.pop_front() {
                self.size_bytes -= front.data.len();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_and_tail() {
        let mut buf = SessionOutputBuffer::new(1024);
        buf.append("hello".to_string());
        buf.append(" world".to_string());

        assert_eq!(buf.size_bytes(), 11); // "hello" (5) + " world" (6)
        assert_eq!(buf.next_sequence(), 2);

        let tail = buf.tail(1024);
        assert_eq!(tail.data, "hello world");
        assert_eq!(tail.seq_start, 0);
        assert_eq!(tail.seq_end, 1);
    }

    #[test]
    fn test_slice_from_sequence() {
        let mut buf = SessionOutputBuffer::new(1024);
        buf.append("first".to_string());   // seq 0
        buf.append("second".to_string());  // seq 1
        buf.append("third".to_string());   // seq 2

        let slice = buf.slice_from_sequence(1);
        assert_eq!(slice.data, "secondthird");
        assert_eq!(slice.seq_start, 1);
        assert_eq!(slice.seq_end, 2);

        // Past all chunks
        let slice = buf.slice_from_sequence(10);
        assert!(slice.data.is_empty());
    }

    #[test]
    fn test_eviction() {
        let mut buf = SessionOutputBuffer::new(10);
        buf.append("0123456789".to_string()); // seq 0, fills buffer
        buf.append("abcdefghij".to_string()); // seq 1, evicts seq 0

        assert_eq!(buf.size_bytes(), 10);
        let tail = buf.tail(100);
        assert_eq!(tail.data, "abcdefghij");
    }

    #[test]
    fn test_oversized_append() {
        let mut buf = SessionOutputBuffer::new(5);
        buf.append("hello world".to_string());
        // Data is truncated to last 5 bytes
        assert_eq!(buf.size_bytes(), 5);
        let tail = buf.tail(100);
        assert_eq!(tail.data, "world");
    }

    #[test]
    fn test_tail_truncation() {
        let mut buf = SessionOutputBuffer::new(1024);
        buf.append("hello world".to_string());
        let tail = buf.tail(5);
        assert_eq!(tail.data, "world");
    }

    #[test]
    fn test_empty_buffer() {
        let buf = SessionOutputBuffer::new(1024);
        assert_eq!(buf.size_bytes(), 0);
        assert_eq!(buf.next_sequence(), 0);
        assert!(buf.latest_sequence().is_none());

        let tail = buf.tail(100);
        assert!(tail.data.is_empty());

        let slice = buf.slice_from_sequence(0);
        assert!(slice.data.is_empty());
    }
}
