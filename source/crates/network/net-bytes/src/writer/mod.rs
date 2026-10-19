//! Writer that reuses its allocations while the messages split from it are alive.
//!
//! Each written message is split off as a [`Bytes`] that shares the writer's chunk, in an
//! [`Arena`]. A chunk is reused once every message split from it is dropped, so a sender whose
//! messages wait for an ack writes into a reclaimed chunk instead of a new allocation.

use crate::varint;
use bytes::Bytes;
use campfire_codec::{Binary, Sink};
use campfire_net_common::Arena;
use serde::Serialize;
use std::io::{Result, Write};

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

impl Write for Writer {
    fn write(&mut self, src: &[u8]) -> Result<usize> {
        self.extend_from_slice(src);
        Ok(src.len())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Sink for Writer {
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

impl Writer {
    /// Appends `value` in `codec`'s binary encoding, the one every message and component takes.
    pub fn encode<T: Serialize + ?Sized>(&mut self, value: &T) {
        Binary::encode_to(value, self);
    }

    /// A writer with one buffer of `capacity` bytes, which grows as plain [`BytesMut`] does.
    ///
    /// [`BytesMut`]: bytes::BytesMut
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

    pub fn extend_from_slice(&mut self, src: &[u8]) {
        self.arena.extend_from_slice(src);
    }

    /// Appends `len` bytes that `fill` writes into the slice it is given, which starts zeroed.
    /// When `fill` fails, the message being written is as it was.
    pub fn fill<E>(
        &mut self,
        len: usize,
        fill: impl FnOnce(&mut [u8]) -> core::result::Result<(), E>,
    ) -> core::result::Result<(), E> {
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

pub trait WriteInteger: Write {
    #[inline]
    fn write_u8(&mut self, n: u8) -> Result<()> {
        self.write_all(&[n])
    }

    #[inline]
    fn write_u16(&mut self, n: u16) -> Result<()> {
        self.write_all(&n.to_be_bytes())
    }

    #[inline]
    fn write_u32(&mut self, n: u32) -> Result<()> {
        self.write_all(&n.to_be_bytes())
    }

    #[inline]
    fn write_u64(&mut self, n: u64) -> Result<()> {
        self.write_all(&n.to_be_bytes())
    }

    /// Write a variable length integer to the writer, in network byte order: the top two bits
    /// of the first byte hold the base-2 logarithm of the length.
    fn write_varint(&mut self, value: u64) -> Result<()> {
        let len = varint::len(value);
        let tag = u64::from(len.trailing_zeros());
        let encoded = (value | (tag << (8 * len - 2))).to_be_bytes();
        self.write_all(&encoded[size_of::<u64>() - len..])
    }
}

impl<T: Write> WriteInteger for T {}

#[cfg(test)]
mod tests;
