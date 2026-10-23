use postcard::ser_flavors::Flavor;
use serde::{Deserialize, Serialize};

use crate::appended::Appended;
use crate::batch::Batch;
use crate::compare::Compare;
use crate::error::{BinaryError, TaggedError};
use crate::sink::Sink;

/// Postcard 1.x, the format of every binary encoding of the engine. A value encodes one way,
/// and decodes only from that way: each decode encodes what it read again, through an output
/// that compares and allocates nothing, and refuses any other bytes.
#[derive(Debug)]
pub struct Binary;

/// A value decoded from the front of bytes, and the bytes after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decoded<'a, T> {
    pub value: T,
    pub rest: &'a [u8],
}

impl Binary {
    /// The bytes of `value`.
    pub fn encode<T: Serialize + ?Sized>(value: &T) -> Vec<u8> {
        let mut out = Vec::new();
        Self::encode_into(value, &mut out);
        out
    }

    /// Appends the bytes of `value` to `out`, in place.
    pub fn encode_into<T: Serialize + ?Sized>(value: &T, out: &mut Vec<u8>) {
        let mut serializer = postcard::Serializer {
            output: Appended(out),
        };
        value
            .serialize(&mut serializer)
            .expect("a value encodes in postcard");
    }

    /// Writes the bytes of `value` into `sink`, in batches; `encode_into` appends to a `Vec` in
    /// place.
    pub fn encode_to<T: Serialize + ?Sized>(value: &T, sink: &mut dyn Sink) {
        Self::encode_each_to([value], sink);
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
        let taken = Self::take(bytes)?;
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
        Self::encode_into(value, out);
    }

    /// The value of a file's `bytes` that `encode_tagged` wrote with `tag`: an error for bytes of
    /// another tag, and for any other value's bytes after it.
    pub fn decode_tagged<'a, T: Deserialize<'a> + Serialize>(
        tag: &[u8],
        bytes: &'a [u8],
    ) -> Result<T, TaggedError> {
        let value = bytes.strip_prefix(tag).ok_or(TaggedError::OtherTag)?;
        Ok(Self::decode(value)?)
    }

    /// The value at the front of `bytes` that another library's encoder wrote, and the bytes
    /// after it, for a type that library gives no `Serialize` to check the bytes against, as
    /// Replicon's diffs: its decode makes no check that the bytes are the value's one encoding.
    pub fn take_unchecked<'a, T: Deserialize<'a>>(
        bytes: &'a [u8],
    ) -> Result<Decoded<'a, T>, BinaryError> {
        let (value, rest) = postcard::take_from_bytes::<T>(bytes)?;
        Ok(Decoded { value, rest })
    }

    /// The value at the front of `bytes`, and the bytes after it.
    pub fn take<'a, T: Deserialize<'a> + Serialize>(
        bytes: &'a [u8],
    ) -> Result<Decoded<'a, T>, BinaryError> {
        let (value, rest) = postcard::take_from_bytes::<T>(bytes)?;
        let read = &bytes[..bytes.len() - rest.len()];
        match postcard::serialize_with_flavor(&value, Compare::new(read)) {
            Ok(matched) if matched == read.len() => Ok(Decoded { value, rest }),
            _ => Err(BinaryError::NotCanonical),
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use postcard::ser_flavors::{AllocVec, Flavor};
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use crate::binary::Binary;
    use crate::drop_option::{DropOption, OptionCount};

    /// An encode with one `Some` written as `None`: its bytes, and how many `Some`s it met.
    #[derive(Debug)]
    struct Dropped {
        bytes: Vec<u8>,
        seen: usize,
    }

    impl Binary {
        /// Whether `value`, encoded, decodes again: a decode keeps the rules a type's own values
        /// follow, as a restore check keeps those that need the match.
        pub fn decodes_again<T: Serialize + DeserializeOwned>(value: &T) -> bool {
            Self::decode::<T>(&Self::encode(value)).is_ok()
        }

        /// How many `Some`s `value` holds, each nested one that its outer one holds among them.
        pub fn options<T: Serialize + ?Sized>(value: &T) -> usize {
            Self::without_option(value, usize::MAX).seen
        }

        /// The bytes of `value` with its `Some` at `at`, in the order it encodes them, written as
        /// `None`, and the options it held left out; none when it holds `at` or fewer.
        pub fn encode_without_option<T: Serialize + ?Sized>(
            value: &T,
            at: usize,
        ) -> Option<Vec<u8>> {
            let dropped = Self::without_option(value, at);
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
    }
}

#[cfg(test)]
mod tests;
