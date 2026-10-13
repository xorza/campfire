use thiserror::Error;
use toml::de;

/// Why bytes are no postcard value of a type. Bytes from a file, a log or the network are
/// untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BinaryError {
    /// They end inside the value.
    #[error("they end inside a value")]
    Truncated,
    /// They are no value of the type, or one its checks refuse.
    #[error("they are no value of the type")]
    Malformed(#[source] postcard::Error),
    /// They decode, but are not the one encoding of the value: postcard's decoder takes an
    /// over-long integer, and a whole value ignores bytes past it.
    #[error("they are not the value's canonical encoding")]
    NotCanonical,
}

impl From<postcard::Error> for BinaryError {
    fn from(error: postcard::Error) -> BinaryError {
        match error {
            postcard::Error::DeserializeUnexpectedEnd => BinaryError::Truncated,
            error => BinaryError::Malformed(error),
        }
    }
}

/// Why a text is no TOML of the type asked for, as `Toml::parse` gives it: TOML's own error,
/// which names the place in the text.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(transparent)]
pub struct TomlError(#[from] de::Error);

impl TomlError {
    /// What is wrong, without the place.
    pub fn message(&self) -> &str {
        self.0.message()
    }
}

/// Why a text is no JSON of the type asked for, as `Json::parse` gives it: JSON's own error,
/// which names the line and column.
#[derive(Debug, Error)]
#[error(transparent)]
pub struct JsonError(#[from] serde_json::Error);
