//! The vocabulary that crates which do not depend on each other share: the player seat, ticks,
//! segment seeds, secret bytes, every 32-byte value by its meaning and the two hashers that make
//! them, a package's fingerprint, a map's name, the TLS key a server presents, and a TLS
//! session's channel binding.
//!
//! A type enters only when two crates that do not depend on each other both name it, and only as a
//! plain value: construction, parsing, display and serde, no other logic. The exceptions are
//! `NamedEnum`, which names an enum's members from one table, with the derives
//! `campfire-common-derive` holds for it; the hashers, SHA-256 and domain-tagged BLAKE3, which
//! construct the 32-byte values (05-protocol-spec.md, Hashes, ids and keys). The crate depends on
//! nothing but `serde`, `derive_more`, `sha2`, `blake3`, `zeroize` and its derives.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

// The derives name the trait by its crate, as every caller does.
#[cfg(test)]
extern crate self as campfire_common;

mod after_leave;
mod bytes32;
mod channel_binding;
mod content_hash;
mod declared_name;
mod domain_hasher;
mod error_report;
mod exit_status;
mod map_name;
mod named_enum;
mod package_fingerprint;
mod seat;
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
pub use crate::declared_name::DeclaredName;
pub use crate::domain_hasher::{DomainHasher, HashDomain, HashOutput};
pub use crate::error_report::ErrorReport;
pub use crate::exit_status::ExitStatus;
pub use crate::map_name::MapName;
pub use crate::map_name::error::NotMapName;
pub use crate::named_enum::NamedEnum;
pub use crate::package_fingerprint::{PackageFingerprint, PackageFingerprintMeaning};
pub use crate::seat::Seat;
pub use crate::secret::Secret;
pub use crate::segment_seed::SegmentSeed;
pub use crate::state_hash::{StateHash, StateHashMeaning};
pub use crate::tick::{Tick, Ticks};
pub use crate::tls_key::{TlsKey, TlsKeyMeaning};
pub use crate::value32::{ContentMeaning, Meaning, Value32};
pub use crate::version::Version;
pub use crate::version::error::NotVersion;
/// The derives of the crate's traits, an enum's `ALL` and `IDENTS`, and `NamedEnum`: the one
/// re-export of another crate the workspace allows (02-engine-core.md, Structural rules), as a
/// proc-macro crate can export nothing else, so a caller takes a trait and its derive from one
/// path, as from `serde`. No other crate names `campfire-common-derive`.
pub use campfire_common_derive::{EnumAll, EnumIdents, NamedEnum};
