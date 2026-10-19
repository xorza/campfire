use crate::reader::{ReadInteger, ReadVarInt, Reader};
use crate::serialization_error::SerializationError;
use crate::var_int::VarInt;
use crate::writer::WriteInteger;
use bytes::Bytes;
use smallvec::{Array, SmallVec};

/// A value the stack's headers write to and read from bytes, whose write cannot fail.
pub trait ToBytes {
    fn bytes_len(&self) -> usize;
    fn to_bytes(&self, buffer: &mut impl WriteInteger);

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized;
}

impl<M: ToBytes> ToBytes for Option<M> {
    fn bytes_len(&self) -> usize {
        match self {
            Some(value) => 1 + value.bytes_len(),
            None => 1,
        }
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        match self {
            Some(value) => {
                buffer.write_u8(1);
                value.to_bytes(buffer);
            }
            None => buffer.write_u8(0),
        }
    }

    /// Refuses a tag other than 0 and 1 (14-workspace-alignment.md, A header decodes from one
    /// encoding).
    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        match buffer.read_u8()? {
            0 => Ok(None),
            1 => Ok(Some(M::from_bytes(buffer)?)),
            _ => Err(SerializationError::InvalidValue),
        }
    }
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
        Ok(buffer.split_len(len)?)
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
        Ok(buffer.read_u8()?)
    }
}

impl ToBytes for u16 {
    fn bytes_len(&self) -> usize {
        VarInt::new(u64::from(*self)).encoded_len()
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        buffer.write_varint(u64::from(*self));
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        Self::try_from(buffer.read_varint()?).map_err(|_| SerializationError::InvalidValue)
    }
}

macro_rules! impl_tuple_query_data {
    ($($name: ident),*) => {

        impl<$($name: ToBytes),*> ToBytes for ($($name,)*) {
            fn bytes_len(&self) -> usize {
                let ($($name,)*) = self;
                let mut len = 0;
                $(len += $name.bytes_len();)*
                len
            }

            fn to_bytes(&self, buffer: &mut impl WriteInteger) {
                let ($($name,)*) = self;
                $($name.to_bytes(buffer);)*
            }

            fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError> {
                Ok(($($name::from_bytes(buffer)?,)*))
            }
        }
    };
}

variadics_please::all_tuples!(impl_tuple_query_data, 1, 8, P);

impl<M: ToBytes> ToBytes for Vec<M> {
    fn bytes_len(&self) -> usize {
        VarInt::new(self.len() as u64).encoded_len()
            + self.iter().map(ToBytes::bytes_len).sum::<usize>()
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        buffer.write_varint(self.len() as u64);
        for item in self {
            item.to_bytes(buffer);
        }
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        let len = buffer.read_count()?;
        let mut vec = Self::with_capacity(len);
        for _ in 0..len {
            vec.push(M::from_bytes(buffer)?);
        }
        Ok(vec)
    }
}

impl<A> ToBytes for SmallVec<A>
where
    A: Array,
    A::Item: ToBytes,
{
    // Match Vec's framing exactly so changing an in-memory collection to SmallVec does not change
    // the wire format.
    fn bytes_len(&self) -> usize {
        VarInt::new(self.len() as u64).encoded_len()
            + self.iter().map(ToBytes::bytes_len).sum::<usize>()
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        buffer.write_varint(self.len() as u64);
        for item in self {
            item.to_bytes(buffer);
        }
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        let len = buffer.read_count()?;
        let mut values = Self::with_capacity(len);
        for _ in 0..len {
            values.push(A::Item::from_bytes(buffer)?);
        }
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::writer::Writer;

    #[test]
    fn a_byte_string_reads_back_as_it_was_written() {
        let a: Bytes = vec![7; 100].into();
        let mut writer = Writer::with_capacity(5);
        a.to_bytes(&mut writer);

        let mut reader = Reader::from(writer.into_bytes());
        let read = Bytes::from_bytes(&mut reader).unwrap();
        assert_eq!(a, read);
    }

    #[test]
    fn a_truncated_value_fails_to_read() {
        let mut writer = Writer::default();
        writer.write_varint(10);
        writer.write_bytes(&[7]);

        let mut reader = Reader::from(writer.into_bytes());
        assert!(Bytes::from_bytes(&mut reader).is_err());
    }

    /// A peer's count beyond the bytes left is refused before any allocation: a varint of
    /// 2^62 - 1 followed by one byte would otherwise reserve exabytes.
    #[test]
    fn counts_beyond_the_remaining_bytes_are_refused() {
        let mut writer = Writer::default();
        writer.write_varint(VarInt::MAX.get());
        writer.write_bytes(&[7]);
        let bytes = writer.into_bytes();
        let truncated = |error: SerializationError| matches!(error, SerializationError::Io(e) if e.kind() == std::io::ErrorKind::UnexpectedEof);

        assert!(truncated(
            Bytes::from_bytes(&mut Reader::from(bytes.clone())).unwrap_err()
        ));
        assert!(truncated(
            Vec::<u8>::from_bytes(&mut Reader::from(bytes.clone())).unwrap_err()
        ));
        assert!(truncated(
            SmallVec::<[u8; 4]>::from_bytes(&mut Reader::from(bytes)).unwrap_err()
        ));
    }

    /// 65535 fits a `u16`; 65536 would alias 0 under truncation.
    #[test]
    fn u16_refuses_a_varint_above_its_range() {
        for (value, expected) in [(65_535, Some(u16::MAX)), (65_536, None)] {
            let mut writer = Writer::default();
            writer.write_varint(value);
            let decoded = u16::from_bytes(&mut Reader::from(writer.into_bytes()));
            match expected {
                Some(expected) => assert_eq!(decoded.unwrap(), expected),
                None => assert!(matches!(decoded, Err(SerializationError::InvalidValue))),
            }
        }
    }

    /// Tag 0 is `None` and tag 1 a value; 2 and 255 would be a second spelling of `Some`.
    #[test]
    fn an_option_reads_only_tags_0_and_1() {
        let decode = |bytes: &[u8]| Option::<u8>::from_bytes(&mut Reader::from(bytes.to_vec()));
        assert_eq!(decode(&[0]).unwrap(), None);
        assert_eq!(decode(&[1, 5]).unwrap(), Some(5));
        for tag in [2, 255] {
            assert!(matches!(
                decode(&[tag, 5]),
                Err(SerializationError::InvalidValue)
            ));
        }
        let mut writer = Writer::default();
        Some(5_u8).to_bytes(&mut writer);
        None::<u8>.to_bytes(&mut writer);
        assert_eq!(&*writer.into_bytes(), [1, 5, 0]);
    }

    #[test]
    fn a_vec_round_trips() {
        let a = [5; 10].to_vec();
        let mut writer = Writer::with_capacity(5);
        a.to_bytes(&mut writer);
        let b: Vec<u8> = vec![];
        b.to_bytes(&mut writer);

        let mut reader = Reader::from(writer.into_bytes());
        let a_read = Vec::<u8>::from_bytes(&mut reader).unwrap();
        let b_read = Vec::<u8>::from_bytes(&mut reader).unwrap();
        assert_eq!(a, a_read);
        assert_eq!(b, b_read);
    }

    #[test]
    fn a_smallvec_takes_the_wire_format_of_a_vec() {
        let vec = (0..6_u8).collect::<Vec<_>>();
        let small = SmallVec::<[u8; 4]>::from_vec(vec.clone());

        let mut vec_writer = Writer::default();
        vec.to_bytes(&mut vec_writer);
        let mut small_writer = Writer::default();
        small.to_bytes(&mut small_writer);
        let vec_bytes = vec_writer.into_bytes();
        let small_bytes = small_writer.into_bytes();
        assert_eq!(vec_bytes, small_bytes);

        let mut reader = Reader::from(small_bytes);
        assert_eq!(SmallVec::<[u8; 4]>::from_bytes(&mut reader).unwrap(), small);
    }
}
