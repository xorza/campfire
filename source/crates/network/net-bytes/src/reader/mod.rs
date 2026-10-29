use bytes::{Buf, Bytes};

use crate::serialization_error::SerializationError;
use crate::var_int::{self, VarInt};

/// The unread bytes of a buffer: each read takes its bytes off the front, and a split shares the
/// buffer's memory (11-net-stack/transport.md, Decoders take only what the bytes hold).
#[derive(Debug, Clone, Default)]
pub struct Reader(Bytes);

impl From<Bytes> for Reader {
    fn from(value: Bytes) -> Self {
        Self(value)
    }
}

impl From<Vec<u8>> for Reader {
    fn from(value: Vec<u8>) -> Self {
        Self(value.into())
    }
}

impl Reader {
    /// The next `len` bytes, split off into a `Bytes` that shares the buffer's memory;
    /// `Truncated` when fewer remain.
    pub fn split_len(&mut self, len: usize) -> Result<Bytes, SerializationError> {
        if len > self.0.len() {
            return Err(SerializationError::Truncated);
        }
        Ok(self.0.split_to(len))
    }

    /// Reads the element count of a collection that follows.
    ///
    /// Every `ToBytes` encoding takes at least one byte, so a count
    /// above the bytes left can not be honest: it is refused before anything is allocated for it.
    pub fn read_count(&mut self) -> Result<usize, SerializationError> {
        let count =
            usize::try_from(self.read_varint()?).map_err(|_| SerializationError::InvalidValue)?;
        if count > self.remaining() {
            return Err(SerializationError::Truncated);
        }
        Ok(count)
    }

    /// The bytes not yet read, which a decoder of its own reads and then `skip`s.
    pub fn rest(&self) -> &[u8] {
        &self.0
    }

    /// Takes the next `len` bytes off the front unread; `Truncated` when fewer remain.
    pub fn skip(&mut self, len: usize) -> Result<(), SerializationError> {
        if len > self.0.len() {
            return Err(SerializationError::Truncated);
        }
        self.0.advance(len);
        Ok(())
    }

    pub const fn has_remaining(&self) -> bool {
        !self.0.is_empty()
    }

    pub const fn remaining(&self) -> usize {
        self.0.len()
    }

    /// The next `N` bytes; `Truncated` when fewer remain.
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], SerializationError> {
        let mut bytes = [0; N];
        if self.0.len() < N {
            return Err(SerializationError::Truncated);
        }
        self.0.copy_to_slice(&mut bytes);
        Ok(bytes)
    }

    pub fn read_u8(&mut self) -> Result<u8, SerializationError> {
        Ok(self.read_array::<1>()?[0])
    }

    pub fn read_u16(&mut self) -> Result<u16, SerializationError> {
        self.read_array().map(u16::from_be_bytes)
    }

    pub fn read_u32(&mut self) -> Result<u32, SerializationError> {
        self.read_array().map(u32::from_be_bytes)
    }

    pub fn read_u64(&mut self) -> Result<u64, SerializationError> {
        self.read_array().map(u64::from_be_bytes)
    }

    /// Reads an unsigned variable-length integer in network byte order, and refuses one longer
    /// than its value needs (11-net-stack/packet-session.md, A header decodes from one encoding).
    pub fn read_varint(&mut self) -> Result<u64, SerializationError> {
        let first = self.read_u8()?;
        let len = var_int::encoded_len_of(first);
        let mut bytes = [0; size_of::<u64>()];
        let start = bytes.len() - len;
        bytes[start] = first & 0x3f;
        let rest = self.split_len(len - 1)?;
        bytes[start + 1..].copy_from_slice(&rest);
        let value = u64::from_be_bytes(bytes);
        if VarInt::new(value).encoded_len() != len {
            return Err(SerializationError::NotCanonical);
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
