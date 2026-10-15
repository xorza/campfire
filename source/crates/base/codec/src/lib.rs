//! The one gateway of serialization (design 02, Serialization): every serde encoding and decoding
//! of the engine, postcard's, TOML's and JSON's, passes through it, so each format has one
//! configuration, one canonical form and one error type. A proxy crate: the formats' calls, which
//! `clippy.toml` denies every other crate, are made here alone.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "the one crate that calls the formats and holds their types, which clippy.toml denies the others"
)]

mod batch;
mod binary;
mod compare;
mod drop_option;
mod error;
mod json;
mod sink;
mod toml;

pub use crate::binary::{Binary, Taken};
pub use crate::error::{BinaryError, JsonError, TaggedError, TomlError};
pub use crate::json::Json;
pub use crate::sink::Sink;
pub use crate::toml::Toml;
