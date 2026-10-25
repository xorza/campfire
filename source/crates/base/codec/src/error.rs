use std::fmt::Display;

use serde::de::Error as DeError;
use thiserror::Error;
use toml::{de, ser};

/// Why bytes are no postcard value of a type. Bytes from a file, a log or the network are
/// untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BinaryError {
    /// They end inside the value.
    #[error("they end inside a value")]
    Truncated,
    #[error(transparent)]
    Malformed(Malformed),
    /// They decode, but are not the one encoding of the value: postcard's decoder takes an
    /// over-long integer, and a whole value ignores bytes past it.
    #[error("they are not the value's canonical encoding")]
    NotCanonical,
}

/// Bytes that are no value of the type, or one its checks refuse: the one case of a
/// `BinaryError` that a reader keeps whole, as it tells the others apart by their own cases.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("they are no value of the type")]
pub struct Malformed(#[source] pub(crate) postcard::Error);

impl From<postcard::Error> for BinaryError {
    fn from(error: postcard::Error) -> Self {
        match error {
            postcard::Error::DeserializeUnexpectedEnd => Self::Truncated,
            error => Self::Malformed(Malformed(error)),
        }
    }
}

/// Why a file's bytes are no tagged file of a value, as `Binary::encode_tagged` writes one.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaggedError {
    /// They do not start with the file's tag: another kind of file, or another version of it.
    #[error("not this kind of file, or another version of it")]
    OtherTag,
    #[error(transparent)]
    Value(#[from] BinaryError),
}

/// Why a text is no TOML of the type asked for, as `Toml::parse` gives it: TOML's own error,
/// which names the place in the text.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(transparent)]
pub struct TomlError(#[from] de::Error);

/// Why a value is none TOML holds, as `TomlValue::try_of` gives it: TOML's own error, which names
/// what it refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(transparent)]
pub struct TomlWriteError(#[from] ser::Error);

/// Why a text is no JSON of the type asked for, as `Json::parse` gives it: JSON's own error,
/// which names the line and column.
#[derive(Debug, Error)]
#[error(transparent)]
pub struct JsonError(#[from] serde_json::Error);

impl TomlError {
    /// The error as another reader's of the format, written whole: TOML's error has no source.
    pub fn into_de<E: DeError>(self) -> E {
        E::custom(self.0.to_string().trim_end())
    }
}

/// The error of a read that a type's own code makes of a kept value, as a registry's read of the
/// value at a key.
impl DeError for TomlError {
    fn custom<T: Display>(message: T) -> Self {
        Self(de::Error::custom(message))
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::error::{BinaryError, Malformed, TomlError};

    impl BinaryError {
        /// The error of bytes of a value whose type's own check refuses it, as its `Deserialize`
        /// makes one: postcard keeps no message of it.
        pub const REFUSED: Self = Self::Malformed(Malformed(postcard::Error::SerdeDeCustom));
    }

    impl TomlError {
        /// What is wrong, without the place.
        pub fn message(&self) -> &str {
            self.0.message()
        }
    }
}

/// Why a PNG file gives no image of 8-bit RGBA pixels.
#[derive(Debug, Error)]
pub enum PngError {
    #[error(transparent)]
    Decode(#[from] png::DecodingError),
    /// It holds pixels other than 8-bit RGBA.
    #[error("the PNG's pixels are not 8-bit RGBA")]
    NotRgba,
}
