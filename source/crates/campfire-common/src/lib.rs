//! The vocabulary that crates which do not depend on each other share: the player slot, ticks,
//! segment seeds and 32-byte values of the session log and the sim, and a package's fingerprint.
//!
//! A type enters only when two crates that do not depend on each other both name it, and only
//! as a plain value: construction, parsing, display and serde, no other logic. The crate
//! depends on nothing but `serde`.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod bytes32;
mod fingerprint;
mod player_slot;
mod segment_seed;
mod state_hash;
mod tick;

pub use bytes32::Bytes32;
pub use bytes32::error::NotHex;
pub use fingerprint::Fingerprint;
pub use player_slot::PlayerSlot;
pub use segment_seed::SegmentSeed;
pub use state_hash::StateHash;
pub use tick::{Tick, Ticks};

#[cfg(test)]
mod tests;
