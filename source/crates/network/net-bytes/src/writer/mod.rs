//! Writer that reuses its allocations while the messages split from it are alive.
//!
//! Each written message is split off as a [`Bytes`] that shares the writer's chunk, in an
//! [`Arena`]. A chunk is reused once every message split from it is dropped, so a sender whose
//! messages wait for an ack writes into a reclaimed chunk instead of a new allocation.

use crate::arena::Arena;
use crate::var_int::VarInt;
use bytes::{Bytes, BytesMut};

/// The size of a chunk. A message that outgrows one gets an allocation of its own.
const CHUNK: usize = 4096;

/// The most chunks a writer keeps for reuse while messages split from them are alive.
const MAX_SPARE_CHUNKS: usize = 4;

#[derive(Debug)]
pub struct Writer {
    arena: Arena,
}

impl Default for Writer {
    fn default() -> Self {
        Self {
            arena: Arena::new(CHUNK, MAX_SPARE_CHUNKS),
        }
    }
}

impl Writer {
    /// A writer with one buffer of `capacity` bytes, which grows as plain [`BytesMut`] does.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            arena: Arena::new(CHUNK, MAX_SPARE_CHUNKS).with_capacity(capacity),
        }
    }

    /// The bytes of the message being written.
    pub fn len(&self) -> usize {
        self.arena.len()
    }

    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    /// Appends `len` bytes that `fill` writes into the slice it is given, which starts zeroed.
    /// When `fill` fails, the message being written is as it was.
    pub fn fill<E>(
        &mut self,
        len: usize,
        fill: impl FnOnce(&mut [u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        let buffer = self.arena.reserve(len);
        let start = buffer.len();
        buffer.resize(start + len, 0);
        let filled = fill(&mut buffer[start..]);
        if filled.is_err() {
            buffer.truncate(start);
        }
        filled
    }

    /// Splits the message written so far off as a separate [`Bytes`]. O(1): the message shares
    /// the writer's chunk.
    pub fn take_written(&mut self) -> Bytes {
        self.arena.split().freeze()
    }

    /// Drops the message being written and keeps the allocation.
    pub fn reset(&mut self) {
        self.arena.reset();
    }

    /// Consumes the writer and returns its written bytes as immutable storage.
    pub fn into_bytes(self) -> Bytes {
        self.arena.into_written().freeze()
    }
}

/// A buffer that takes the stack's header encodings, big-endian integers and varints. A write
/// cannot fail: each buffer grows to take it.
pub trait WriteInteger {
    fn write_bytes(&mut self, bytes: &[u8]);

    #[inline]
    fn write_u8(&mut self, n: u8) {
        self.write_bytes(&[n]);
    }

    #[inline]
    fn write_u16(&mut self, n: u16) {
        self.write_bytes(&n.to_be_bytes());
    }

    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.write_bytes(&n.to_be_bytes());
    }

    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.write_bytes(&n.to_be_bytes());
    }

    /// Writes `value` as a variable-length integer in network byte order, in the fewest bytes
    /// that hold it: the top two bits of the first byte hold the base-2 logarithm of the length.
    fn write_varint(&mut self, value: u64) {
        let len = VarInt::new(value).encoded_len();
        let tag = u64::from(len.trailing_zeros());
        let encoded = (value | (tag << (8 * len - 2))).to_be_bytes();
        self.write_bytes(&encoded[size_of::<u64>() - len..]);
    }
}

impl WriteInteger for Writer {
    fn write_bytes(&mut self, bytes: &[u8]) {
        self.arena.extend_from_slice(bytes);
    }
}

impl WriteInteger for Vec<u8> {
    fn write_bytes(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

impl WriteInteger for BytesMut {
    fn write_bytes(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

#[cfg(test)]
mod tests;
