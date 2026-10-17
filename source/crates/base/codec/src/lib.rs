//! The one gateway of serialization (02-engine-core.md, Serialization): every serde encoding and
//! decoding of the engine, postcard's, TOML's and JSON's, passes through it, so each format has one
//! configuration, one canonical form and one error type. A proxy crate: the formats' calls, which
//! `clippy.toml` denies every other crate, are made here alone.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (09-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "the one crate that calls the formats and holds their values, which clippy.toml denies the others"
)]

mod batch;
mod binary;
mod compare;
#[cfg(feature = "internals")]
mod drop_option;
mod error;
mod json;
mod json_value;
mod sink;
mod toml;
#[cfg(feature = "internals")]
mod toml_document;
mod toml_table;

pub use crate::binary::{Binary, Taken};
pub use crate::error::{BinaryError, JsonError, TaggedError, TomlError};
pub use crate::json::Json;
pub use crate::json_value::JsonValue;
pub use crate::sink::Sink;
pub use crate::toml::Toml;
pub use crate::toml_table::TomlTable;

/// What the tests of the engine's crates use of the formats: a TOML document they edit, and its
/// values.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::toml_document::{TomlDocument, TomlNode};
}
