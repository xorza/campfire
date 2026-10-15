//! The vocabulary that crates which do not depend on each other share: the player slot, ticks,
//! segment seeds, secret bytes and 32-byte values of the session log and the sim, a package's
//! fingerprint, a map's name, the TLS key a server presents, and a TLS session's channel
//! binding.
//!
//! A type enters only when two crates that do not depend on each other both name it, and only
//! as a plain value: construction, parsing, display and serde, no other logic. The exceptions
//! are `enum_all!`, which states an enum with the list of its variants, and, behind `internals`,
//! the allocation audit every crate's `tests/alloc` suite runs on. The crate depends on nothing
//! but `serde` and `derive_more`, and, for the audit, `backtrace`.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod after_leave;
#[cfg(feature = "internals")]
mod alloc_audit;
mod bytes32;
mod channel_binding;
mod enum_all;
mod exit_status;
mod fingerprint;
mod map_name;
mod player_slot;
mod secret;
mod segment_seed;
mod state_hash;
mod tick;
mod tls_key;
mod version;

pub use crate::after_leave::AfterLeave;
pub use crate::bytes32::Bytes32;
pub use crate::bytes32::error::NotHex;
pub use crate::channel_binding::ChannelBinding;
pub use crate::exit_status::ExitStatus;
pub use crate::fingerprint::Fingerprint;
pub use crate::map_name::MapName;
pub use crate::map_name::error::NotMapName;
pub use crate::player_slot::PlayerSlot;
pub use crate::secret::Secret;
pub use crate::segment_seed::SegmentSeed;
pub use crate::state_hash::StateHash;
pub use crate::tick::{Tick, Ticks};
pub use crate::tls_key::TlsKey;
pub use crate::version::Version;
pub use crate::version::error::NotVersion;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::alloc_audit::counting_allocator::{AllocWindow, CountingAllocator};
    pub use crate::alloc_audit::{AllocAudit, AuditReport};
}

#[cfg(test)]
mod tests;
