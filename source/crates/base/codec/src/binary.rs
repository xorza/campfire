use std::mem;

use postcard::ser_flavors::{AllocVec, Flavor};
use serde::{Deserialize, Serialize};

use crate::batch::Batch;
use crate::compare::Compare;
use crate::drop_option::{DropOption, OptionCount};
use crate::error::{BinaryError, TaggedError};
use crate::sink::Sink;

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

    /// How many `Some`s `value` holds, each nested one that its outer one holds among them.
    pub fn options<T: Serialize + ?Sized>(value: &T) -> usize {
        Binary::without_option(value, usize::MAX).seen
    }

    /// The bytes of `value` with its `Some` at `at`, in the order it encodes them, written as
    /// `None`, and the options it held left out; none when it holds `at` or fewer.
    pub fn encode_without_option<T: Serialize + ?Sized>(value: &T, at: usize) -> Option<Vec<u8>> {
        let dropped = Binary::without_option(value, at);
        (dropped.seen > at).then_some(dropped.bytes)
    }

    fn without_option<T: Serialize + ?Sized>(value: &T, at: usize) -> Dropped {
        let mut serializer = postcard::Serializer {
            output: AllocVec::new(),
        };
        let mut count = OptionCount {
            seen: 0,
            dropped: at,
        };
        value
            .serialize(DropOption {
                inner: &mut serializer,
                count: &mut count,
            })
            .expect("a value encodes in postcard");
        let bytes = serializer
            .output
            .finalize()
            .expect("an encode into a vec cannot fail");
        Dropped {
            bytes,
            seen: count.seen,
        }
    }

    /// The value `bytes` hold, all of them.
    pub fn decode<'a, T: Deserialize<'a> + Serialize>(bytes: &'a [u8]) -> Result<T, BinaryError> {
        let taken = Binary::take(bytes)?;
        if !taken.rest.is_empty() {
            return Err(BinaryError::NotCanonical);
        }
        Ok(taken.value)
    }

    /// The bytes of a file of `value` into `out`, in place of what it held: `tag`, which names
    /// the kind of file and its version, then the value.
    pub fn encode_tagged<T: Serialize + ?Sized>(tag: &[u8], value: &T, out: &mut Vec<u8>) {
        out.clear();
        out.extend_from_slice(tag);
        Binary::encode_into(value, out);
    }

    /// The value of a file's `bytes` that `encode_tagged` wrote with `tag`: an error for bytes of
    /// another tag, and for any other value's bytes after it.
    pub fn decode_tagged<'a, T: Deserialize<'a> + Serialize>(
        tag: &[u8],
        bytes: &'a [u8],
    ) -> Result<T, TaggedError> {
        let value = bytes.strip_prefix(tag).ok_or(TaggedError::OtherTag)?;
        Ok(Binary::decode(value)?)
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

/// An encode with one `Some` written as `None`: its bytes, and how many `Some`s it met.
#[derive(Debug)]
struct Dropped {
    bytes: Vec<u8>,
    seen: usize,
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
    fn a_tagged_file_reads_back_only_under_its_tag_and_whole() {
        // The tag `t/v1`, then 300, `0xAC 0x02`, written in place of what `out` held.
        let mut out = vec![9; 4];
        Binary::encode_tagged(b"t/v1", &300_u32, &mut out);
        assert_eq!(out, b"t/v1\xAC\x02");
        assert_eq!(Binary::decode_tagged::<u32>(b"t/v1", &out), Ok(300));
        assert_eq!(
            Binary::decode_tagged::<u32>(b"t/v2", &out),
            Err(TaggedError::OtherTag)
        );
        let short = &out[..out.len() - 1];
        assert_eq!(
            Binary::decode_tagged::<u32>(b"t/v1", short),
            Err(TaggedError::Value(BinaryError::Truncated))
        );
        let mut long = out.clone();
        long.push(0);
        assert_eq!(
            Binary::decode_tagged::<u32>(b"t/v1", &long),
            Err(TaggedError::Value(BinaryError::NotCanonical))
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

    /// A value with options at each depth: a field, a tuple's inside a `Some`, a vector's
    /// element, an enum's variant, and a struct's.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Nested {
        first: Option<u8>,
        pair: Option<(Option<u8>, u16)>,
        list: Vec<Option<u32>>,
        kind: Kind,
        none: Option<u8>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    enum Kind {
        Plain,
        With { rank: Option<u8> },
    }

    #[test]
    fn an_encode_writes_each_some_in_turn_as_none_and_the_rest_as_it_is() {
        let value = Nested {
            first: Some(1),
            pair: Some((Some(2), 3)),
            list: vec![Some(4), None, Some(5)],
            kind: Kind::With { rank: Some(6) },
            none: None,
        };
        // The `Some`s in order: `first`, `pair`, its inner one, the list's two, the rank.
        assert_eq!(Binary::options(&value), 6);
        let dropped = |at| {
            let bytes = Binary::encode_without_option(&value, at)?;
            Some(Binary::decode::<Nested>(&bytes).unwrap())
        };
        let with = |change: fn(&mut Nested)| {
            let mut changed = value.clone();
            change(&mut changed);
            Some(changed)
        };
        assert_eq!(dropped(0), with(|v| v.first = None));
        // The outer `None` takes the inner `Some` with it.
        assert_eq!(dropped(1), with(|v| v.pair = None));
        assert_eq!(dropped(2), with(|v| v.pair = Some((None, 3))));
        assert_eq!(dropped(3), with(|v| v.list[0] = None));
        assert_eq!(dropped(4), with(|v| v.list[2] = None));
        assert_eq!(dropped(5), with(|v| v.kind = Kind::With { rank: None }));
        assert_eq!(dropped(6), None);
        // A value with no `Some` has nothing to write as `None`.
        let plain = (Kind::Plain, None::<u8>, vec![None::<u8>]);
        assert_eq!(Binary::options(&plain), 0);
        assert_eq!(Binary::encode_without_option(&plain, 0), None);
    }
}
