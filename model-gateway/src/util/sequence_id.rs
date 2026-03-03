use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Epoch start: 1 Jan 2001 00:00:00 UTC
const EPOCH_OFFSET: u64 = 978307200;

/// Custom sequence ID generator
/// - 42 bits: time from 1 Jan 2001 UTC
/// - 8 bits: node ID (configurable)
/// - 8 bits: atomic counter (resets when time changes)
/// - 6 bits: random
pub struct SequenceId {
    node_id: u8,
    counter: AtomicU64,
}

impl SequenceId {
    /// Creates a new SequenceId with default node_id
    pub fn new() -> Self {
        Self::with_node_id(0)
    }

    /// Creates a new SequenceId with custom node_id
    pub fn with_node_id(node_id: u8) -> Self {
        Self {
            node_id,
            counter: AtomicU64::new(0),
        }
    }

    /// Generates a new 64-bit ID
    pub fn next_id(&self) -> u64 {
        // * get current time in milliseconds since epoch
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // * calculate time since custom epoch (1 Jan 2001) in milliseconds
        let time = now.saturating_sub(EPOCH_OFFSET * 1000);

        // * get and increment counter, reset to random if overflow
        let counter = self.counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |c| {
            if c >= 255 {
                Some(rand_simple() as u64 & 0xFF)
            } else {
                Some(c + 1)
            }
        }).unwrap_or(0);

        // * get random bits
        let random = (rand_simple() & 0x3F) as u64; // 6 bits

        // * assemble the ID
        // 42 bits time | 8 bits node | 8 bits counter | 6 bits random
        let id = (time & 0x3FFFFFFFFFF) << 22      // 42 bits time
            | ((self.node_id as u64) & 0xFF) << 14      // 8 bits node
            | (counter & 0xFF) << 6                     // 8 bits counter
            | random;                                   // 6 bits random

        id
    }
}

impl Default for SequenceId {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple random number generator (xorshift)
fn rand_simple() -> u32 {
    use std::time::SystemTime;
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u32;

    let mut x = seed.wrapping_mul(1103515245).wrapping_add(12345);
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_id() {
        let id_gen = SequenceId::new();
        let id1 = id_gen.next_id();
        let id2 = id_gen.next_id();

        assert_ne!(id1, id2);
        println!("ID1: {:064b}", id1);
        println!("ID2: {:064b}", id2);
    }

    #[test]
    fn test_different_node_ids() {
        let id_gen1 = SequenceId::with_node_id(1);
        let id_gen2 = SequenceId::with_node_id(2);

        let id1 = id_gen1.next_id();
        let id2 = id_gen2.next_id();

        // * extract node bits (bits 14-21)
        let node1 = (id1 >> 14) & 0xFF;
        let node2 = (id2 >> 14) & 0xFF;

        assert_eq!(node1, 1);
        assert_eq!(node2, 2);
    }
}
