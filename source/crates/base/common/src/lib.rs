//! The vocabulary that crates which do not depend on each other share: the player slot, ticks,
//! segment seeds, secret bytes, every 32-byte value by its meaning and the two hashers that make
//! them, a package's fingerprint, a map's name, the TLS key a server presents, and a TLS
//! session's channel binding.
//!
//! A type enters only when two crates that do not depend on each other both name it, and only
//! as a plain value: construction, parsing, display and serde, no other logic. The exceptions
//! are `enum_all!`, which states an enum with the list of its variants; the hashers, SHA-256 and
//! domain-tagged BLAKE3, which construct the 32-byte values (design 05, Hashes, ids and keys);
//! and, behind `internals`, the allocation audit every crate's `tests/alloc` suite runs on. The
//! crate depends on nothing but `serde`, `derive_more`, `sha2` and `blake3`, and, for the audit,
//! `backtrace`.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod after_leave;
#[cfg(feature = "internals")]
mod alloc_audit;
mod bytes32;
mod channel_binding;
mod content_hash;
mod domain_hasher;
mod enum_all;
mod exit_status;
mod map_name;
mod package_fingerprint;
mod player_slot;
mod secret;
mod segment_seed;
mod state_hash;
mod tick;
mod tls_key;
mod value32;
mod version;

pub use crate::after_leave::AfterLeave;
pub use crate::bytes32::Bytes32;
pub use crate::bytes32::error::NotHex;
pub use crate::channel_binding::{ChannelBinding, ChannelBindingMeaning};
pub use crate::content_hash::{ContentHash, ContentHashMeaning, ContentHasher};
pub use crate::domain_hasher::{DomainHasher, HashDomain};
pub use crate::exit_status::ExitStatus;
pub use crate::map_name::MapName;
pub use crate::map_name::error::NotMapName;
pub use crate::package_fingerprint::{PackageFingerprint, PackageFingerprintMeaning};
pub use crate::player_slot::PlayerSlot;
pub use crate::secret::Secret;
pub use crate::segment_seed::SegmentSeed;
pub use crate::state_hash::{StateHash, StateHashMeaning};
pub use crate::tick::{Tick, Ticks};
pub use crate::tls_key::{TlsKey, TlsKeyMeaning};
pub use crate::value32::{ContentMeaning, Meaning, Value32};
pub use crate::version::Version;
pub use crate::version::error::NotVersion;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::alloc_audit::counting_allocator::{AllocWindow, CountingAllocator};
    pub use crate::alloc_audit::{AllocAudit, AuditReport};
}

#[cfg(test)]
mod tests;
