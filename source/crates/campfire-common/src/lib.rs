//! The vocabulary that crates which do not depend on each other share: the player slot, ticks,
//! segment seeds, secret bytes and 32-byte values of the session log and the sim, a package's
//! fingerprint, and a map's name.
//!
//! A type enters only when two crates that do not depend on each other both name it, and only
//! as a plain value: construction, parsing, display and serde, no other logic. The one exception
//! is `codec`, the gateway every crate serializes through (design 02, Serialization). The crate
//! depends on nothing but `serde`, `derive_more` and `thiserror`, and, for `codec`, the formats it
//! owns and `arrayvec`.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod bytes32;
mod codec;
mod exit_status;
mod fingerprint;
mod map_name;
mod player_slot;
mod secret;
mod segment_seed;
mod state_hash;
mod tick;

pub use crate::bytes32::Bytes32;
pub use crate::bytes32::error::NotHex;
pub use crate::codec::binary::{Binary, Taken};
pub use crate::codec::error::BinaryError;
pub use crate::codec::json::Json;
pub use crate::codec::sink::Sink;
pub use crate::codec::toml::Toml;
pub use crate::exit_status::ExitStatus;
pub use crate::fingerprint::Fingerprint;
pub use crate::map_name::MapName;
pub use crate::map_name::error::NotMapName;
pub use crate::player_slot::PlayerSlot;
pub use crate::secret::Secret;
pub use crate::segment_seed::SegmentSeed;
pub use crate::state_hash::StateHash;
pub use crate::tick::{Tick, Ticks};

#[cfg(test)]
mod tests;
