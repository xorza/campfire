//! The network stack's header codec: a writer that reuses its chunks, a reader, varints, and
//! `ToBytes` for the stack's own headers, whose writes cannot fail.

mod arena;
#[cfg(feature = "internals")]
pub mod bench;
#[cfg(feature = "internals")]
mod mutations;
mod reader;
mod serialization_error;
mod to_bytes;
mod var_int;
mod wrapping_id;
mod writer;

pub use crate::arena::Arena;
pub use crate::reader::ReadInteger;
pub use crate::reader::ReadVarInt;
pub use crate::reader::Reader;
pub use crate::serialization_error::SerializationError;
pub use crate::to_bytes::ToBytes;
pub use crate::var_int::VarInt;
pub use crate::writer::WriteInteger;
pub use crate::writer::Writer;

/// What the tests of the stack's parsers share: the seeded mutations of a valid input.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::mutations::Mutations;
}
