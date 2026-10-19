//! The network stack's byte buffers: a writer that reuses its chunks, a reader, varints, and
//! `ToBytes` for the stack's own headers.
//! It defines the [`ToBytes`] trait for converting types to and from byte representations,
//! along with [`Reader`] and [`Writer`] utilities for handling byte streams.
//!
//! It includes implementations for common types and collections, and utilities for
//! efficient serialization, such as varint encoding.
//!
//! This crate is fundamental for preparing data to be sent over the network and for
//! reconstructing data received from remote peers.

#[cfg(feature = "internals")]
pub mod bench;
mod reader;
mod serialization_error;
mod to_bytes;
/// Utilities for variable-length integer encoding and decoding.
pub mod varint;
mod wrapping_id;
mod writer;

pub use crate::reader::ReadInteger;
pub use crate::reader::ReadVarInt;
pub use crate::reader::Reader;
pub use crate::serialization_error::SerializationError;
pub use crate::to_bytes::ToBytes;
pub use crate::writer::WriteInteger;
pub use crate::writer::Writer;
