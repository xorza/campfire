use crate::reader::Reader;
use crate::serialization_error::SerializationError;
use crate::var_int::VarInt;
use crate::writer::WriteInteger;
use bytes::Bytes;

/// A value the stack's headers write to and read from bytes, whose write cannot fail.
pub trait ToBytes {
    fn bytes_len(&self) -> usize;
    fn to_bytes(&self, buffer: &mut impl WriteInteger);

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized;
}

/// When we read, instead of allocating we just create a new Bytes by slicing the buffer
impl ToBytes for Bytes {
    fn bytes_len(&self) -> usize {
        VarInt::new(self.len() as u64).encoded_len() + self.len()
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        buffer.write_varint(self.len() as u64);
        buffer.write_bytes(self.as_ref());
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        let len = buffer.read_count()?;
        buffer.split_len(len)
    }
}

impl ToBytes for u8 {
    fn bytes_len(&self) -> usize {
        1
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        buffer.write_u8(*self);
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        buffer.read_u8()
    }
}

#[cfg(test)]
mod tests;
