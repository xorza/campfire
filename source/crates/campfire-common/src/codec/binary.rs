use std::mem;

use postcard::ser_flavors::Flavor;
use serde::{Deserialize, Serialize};

use crate::codec::batch::Batch;
use crate::codec::compare::Compare;
use crate::codec::error::BinaryError;
use crate::codec::sink::Sink;

/// Postcard 1.x, the format of every binary encoding of the engine. A value encodes one way,
/// and decodes only from that way: each decode encodes what it read again, through an output
/// that compares and allocates nothing, and refuses any other bytes.
#[derive(Debug)]
pub struct Binary;

/// A value decoded from the front of bytes, and the bytes after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Taken<'a, T> {
    pub value: T,
    pub rest: &'a [u8],
}

impl Binary {
    /// The bytes of `value`.
    pub fn encode<T: Serialize + ?Sized>(value: &T) -> Vec<u8> {
        postcard::to_allocvec(value).expect("a value encodes in postcard")
    }

    /// Appends the bytes of `value` to `out`.
    pub fn encode_into<T: Serialize + ?Sized>(value: &T, out: &mut Vec<u8>) {
        *out = postcard::to_extend(value, mem::take(out)).expect("a value encodes in postcard");
    }

    /// Writes the bytes of `value` into `sink`, in batches.
    pub fn encode_to<T: Serialize + ?Sized>(value: &T, sink: &mut dyn Sink) {
        Binary::encode_each_to([value], sink);
    }

    /// Writes the bytes of each of `values` into `sink`, one after another, in batches.
    pub fn encode_each_to<T: Serialize>(values: impl IntoIterator<Item = T>, sink: &mut dyn Sink) {
        let mut serializer = postcard::Serializer {
            output: Batch::new(sink),
        };
        for value in values {
            value
                .serialize(&mut serializer)
                .expect("a value encodes in postcard");
        }
        serializer
            .output
            .finalize()
            .expect("a batch into a sink cannot fail");
    }

    /// The value `bytes` hold, all of them.
    pub fn decode<'a, T: Deserialize<'a> + Serialize>(bytes: &'a [u8]) -> Result<T, BinaryError> {
        let taken = Binary::take(bytes)?;
        if !taken.rest.is_empty() {
            return Err(BinaryError::NotCanonical);
        }
        Ok(taken.value)
    }

    /// The value at the front of `bytes`, and the bytes after it.
    pub fn take<'a, T: Deserialize<'a> + Serialize>(
        bytes: &'a [u8],
    ) -> Result<Taken<'a, T>, BinaryError> {
        let (value, rest) = postcard::take_from_bytes::<T>(bytes)?;
        let read = &bytes[..bytes.len() - rest.len()];
        match postcard::serialize_with_flavor(&value, Compare::new(read)) {
            Ok(matched) if matched == read.len() => Ok(Taken { value, rest }),
            _ => Err(BinaryError::NotCanonical),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_decodes_only_from_its_one_encoding() {
        // 300 is the varint 0xAC 0x02: its low seven bits with the high bit set, then 300 >> 7.
        assert_eq!(Binary::encode(&300_u32), [0xAC, 0x02]);
        assert_eq!(Binary::decode::<u32>(&[0xAC, 0x02]), Ok(300));
        // 0 over-long, 0x80 0x00; a byte past the value; bytes that end inside it; a bool of 2.
        assert_eq!(
            Binary::decode::<u32>(&[0x80, 0x00]),
            Err(BinaryError::NotCanonical)
        );
        assert_eq!(
            Binary::decode::<u32>(&[0x01, 0xFF]),
            Err(BinaryError::NotCanonical)
        );
        assert_eq!(Binary::decode::<u32>(&[0xAC]), Err(BinaryError::Truncated));
        assert_eq!(
            Binary::decode::<bool>(&[2]),
            Err(BinaryError::Malformed(postcard::Error::DeserializeBadBool))
        );
        // A length over-long inside a value: the string "ab" as 0x82 0x00 'a' 'b'.
        assert_eq!(
            Binary::decode::<&str>(&[0x82, 0x00, b'a', b'b']),
            Err(BinaryError::NotCanonical)
        );
        assert_eq!(Binary::decode::<&str>(&[2, b'a', b'b']), Ok("ab"));
    }

    #[test]
    fn a_value_taken_from_the_front_gives_the_rest() {
        let bytes = [2, b'a', b'b', 0xAC, 0x02];
        let first = Binary::take::<&str>(&bytes).unwrap();
        assert_eq!(first.value, "ab");
        assert_eq!(first.rest, [0xAC, 0x02]);
        let second = Binary::take::<u32>(first.rest).unwrap();
        assert_eq!((second.value, second.rest), (300, &[][..]));
        // An over-long value at the front is refused, whatever follows it.
        assert_eq!(
            Binary::take::<u32>(&[0x80, 0x00, 0x05]),
            Err(BinaryError::NotCanonical)
        );
    }

    #[test]
    fn every_encode_writes_the_same_bytes() {
        // A value longer than a batch of 64, so the sink takes it in more than one call.
        let value = (vec![7_u8; 100], 300_u32, "end");
        let bytes = Binary::encode(&value);
        assert_eq!(bytes.len(), 1 + 100 + 2 + 1 + 3);
        let mut appended = vec![9];
        Binary::encode_into(&value, &mut appended);
        assert_eq!(appended, [&[9][..], &bytes].concat());
        let mut sunk = Vec::new();
        Binary::encode_to(&value, &mut sunk);
        assert_eq!(sunk, bytes);
        let mut each = Vec::new();
        Binary::encode_each_to([1_u8, 2, 3], &mut each);
        assert_eq!(each, [1, 2, 3]);
        assert_eq!(Binary::decode::<(Vec<u8>, u32, &str)>(&bytes), Ok(value));
    }
}
