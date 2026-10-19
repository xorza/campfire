use crate::serialization_error::SerializationError;
use crate::var_int::{self, VarInt};
use bytes::{Buf, Bytes};
use std::io::{Error, ErrorKind, Read, Result};

/// The unread bytes of a buffer: each read takes its bytes off the front, and a split shares the
/// buffer's memory.
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

impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let len = buf.len().min(self.0.len());
        self.0.copy_to_slice(&mut buf[..len]);
        Ok(len)
    }
}

impl Buf for Reader {
    fn remaining(&self) -> usize {
        self.0.len()
    }

    fn chunk(&self) -> &[u8] {
        &self.0
    }

    fn advance(&mut self, cnt: usize) {
        self.0.advance(cnt);
    }
}

impl Reader {
    /// Split of the next `len` bytes from the reader into a separate Bytes.
    ///
    /// This doesn't allocate and just increases some reference counts. O(1) cost.
    /// Returns [`ErrorKind::UnexpectedEof`] if fewer than `len` bytes remain.
    pub fn split_len(&mut self, len: usize) -> Result<Bytes> {
        if len > self.0.len() {
            return Err(Error::from(ErrorKind::UnexpectedEof));
        }
        Ok(self.0.split_to(len))
    }

    /// Return the remaining length of the buffer as a separate Bytes.
    ///
    /// This doesn't allocate and just increases some reference counts. O(1) cost.
    pub fn take_remaining(&mut self) -> Bytes {
        core::mem::take(&mut self.0)
    }

    /// Reads the element count of a collection that follows.
    ///
    /// Every `ToBytes` encoding takes at least one byte, so a count
    /// above the bytes left can not be honest: it is refused before anything is allocated for it.
    pub fn read_count(&mut self) -> core::result::Result<usize, SerializationError> {
        let count =
            usize::try_from(self.read_varint()?).map_err(|_| SerializationError::InvalidValue)?;
        if count > self.remaining() {
            return Err(Error::from(ErrorKind::UnexpectedEof).into());
        }
        Ok(count)
    }

    pub const fn has_remaining(&self) -> bool {
        !self.0.is_empty()
    }

    pub const fn remaining(&self) -> usize {
        self.0.len()
    }
}

pub trait ReadInteger: Read {
    #[inline]
    fn read_u8(&mut self) -> Result<u8> {
        let mut buf = [0; 1];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    #[inline]
    fn read_u16(&mut self) -> Result<u16> {
        let mut buf = [0; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_be_bytes(buf))
    }

    #[inline]
    fn read_u32(&mut self) -> Result<u32> {
        let mut buf = [0; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }

    #[inline]
    fn read_u64(&mut self) -> Result<u64> {
        let mut buf = [0; 8];
        self.read_exact(&mut buf)?;
        Ok(u64::from_be_bytes(buf))
    }
}

pub trait ReadVarInt: ReadInteger {
    /// Reads an unsigned variable-length integer in network byte order, and refuses one longer
    /// than its value needs (14-workspace-alignment.md, A header decodes from one encoding).
    fn read_varint(&mut self) -> core::result::Result<u64, SerializationError> {
        let first = self.read_u8()?;
        let len = var_int::encoded_len_of(first);
        let mut bytes = [0; size_of::<u64>()];
        let start = bytes.len() - len;
        bytes[start] = first & 0x3f;
        self.read_exact(&mut bytes[start + 1..])?;
        let value = u64::from_be_bytes(bytes);
        if VarInt::new(value).encoded_len() != len {
            return Err(SerializationError::NotCanonical);
        }
        Ok(value)
    }
}

impl<T: Read> ReadInteger for T {}
impl<T: Read> ReadVarInt for T {}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::writer::WriteInteger;

    #[test]
    fn integers_read_back() {
        let mut writer = vec![];
        writer.write_u8(1);
        writer.write_u16(2);
        writer.write_u32(3);
        writer.write_u64(4);

        let mut reader = Reader::from(writer);

        assert_eq!(reader.read_u8().unwrap(), 1);
        assert_eq!(reader.read_u16().unwrap(), 2);
        assert_eq!(reader.read_u32().unwrap(), 3);
        assert_eq!(reader.read_u64().unwrap(), 4);
        assert_eq!(reader.remaining(), 0);
        assert!(reader.read_u8().is_err());
    }

    /// A split takes the next bytes and leaves the rest, a split past the end is refused and takes
    /// nothing, and the rest comes whole.
    #[test]
    fn a_split_takes_the_next_bytes() {
        let mut reader = Reader::from(vec![1, 2, 3, 4, 5]);
        assert_eq!(reader.split_len(2).unwrap(), [1, 2].as_slice());
        assert!(reader.split_len(4).is_err());
        assert_eq!(reader.remaining(), 3);
        let mut byte = [0];
        assert_eq!(reader.read(&mut byte).unwrap(), 1);
        assert_eq!(byte, [3]);
        assert_eq!(reader.take_remaining(), [4, 5].as_slice());
        assert!(!reader.has_remaining());
    }

    /// Three bytes follow the count: a count of 3 is the largest the buffer can hold.
    #[test]
    fn read_count_allows_up_to_the_remaining_bytes() {
        for (count, fits) in [(0, true), (3, true), (4, false)] {
            let mut writer = vec![];
            writer.write_varint(count);
            writer.extend_from_slice(&[1, 2, 3]);
            let mut reader = Reader::from(writer);
            let read = reader.read_count();
            if fits {
                assert_eq!(read.unwrap(), usize::try_from(count).unwrap());
                assert_eq!(reader.remaining(), 3);
            } else {
                assert!(read.is_err());
            }
        }
    }

    #[test]
    fn a_varint_reads_back_without_seek() {
        let values = [
            0,
            63,
            64,
            16_383,
            16_384,
            1_073_741_823,
            1_073_741_824,
            4_611_686_018_427_387_903,
        ];
        let mut writer = vec![];
        for value in values {
            writer.write_varint(value);
        }

        let mut reader = writer.as_slice();
        for value in values {
            assert_eq!(reader.read_varint().unwrap(), value);
        }
    }
}
