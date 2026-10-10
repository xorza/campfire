//! The one gateway of serialization (design 02, Serialization): every serde encoding and decoding
//! of the engine, postcard's, TOML's and JSON's, passes through it, so each format has one
//! configuration, one canonical form and one error type.

#![expect(
    clippy::disallowed_methods,
    reason = "the one module that calls the formats, which clippy.toml denies the others"
)]

pub(crate) mod batch;
pub(crate) mod binary;
pub(crate) mod compare;
pub(crate) mod error;
pub(crate) mod json;
pub(crate) mod sink;
pub(crate) mod toml;
