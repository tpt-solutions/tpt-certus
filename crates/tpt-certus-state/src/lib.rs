//! Deterministic state management, cryptographic hashing, and temporal replay.
//!
//! [`tpt-certus-state`] provides:
//!
//! * **Deterministic state serialization:** spatial state can be serialized to
//!   a canonical byte representation that is bit-exact across platforms.
//! * **Cryptographic state hashing:** a per-timestep hash that serves as a
//!   replay anchor and tamper-evident log entry.
//! * **Temporal replay/audit:** ordered playback of state transitions with
//!   cryptographic verification of each step.
//!
//! # Two-layer role
//!
//! Ideal-layer contracts prove lossless serialization (bijection between state
//! and its canonical encoding).  Realization-layer contracts prove the f64
//! serialization matches the ideal encoding within a deterministic bound.
//!
//! # Status
//!
//! Phase 0 scaffold only — types and contracts added in a later phase.

#![no_std]

extern crate alloc;

/// A state snapshot identified by its content hash.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StateSnapshot {
    /// Monotonically increasing sequence number (deterministic across runs).
    pub sequence: u64,
    /// SHA-256 hash of the canonical state encoding.
    pub hash: [u8; 32],
}

impl StateSnapshot {
    /// Create a new snapshot placeholder.  Phase 1 will replace the hash
    /// computation with a verified canonical-encoding + hash pipeline.
    pub fn new(sequence: u64, hash: [u8; 32]) -> Self {
        StateSnapshot { sequence, hash }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_equality() {
        let hash = [0xAA; 32];
        let s1 = StateSnapshot::new(1, hash);
        let s2 = StateSnapshot::new(1, hash);
        assert_eq!(s1, s2);
    }

    #[test]
    fn snapshot_sequence_ordering() {
        let h = [0u8; 32];
        let s1 = StateSnapshot::new(0, h);
        let s2 = StateSnapshot::new(1, h);
        assert!(s1.sequence < s2.sequence);
    }
}
