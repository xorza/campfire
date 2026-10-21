use std::fmt;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::JsonError;

/// A JSON value read and kept whole, to read again as a type, or by the text of one of its
/// members: as a log line's fields, which each event reads back as its own type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct JsonValue(serde_json::Value);

impl JsonValue {
    /// The text of member `key`; none for a value of no such member, or one that is no text.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.0.get(key)?.as_str()
    }

    /// The value as a `T`; an error when it is no `T`.
    pub fn read<T: DeserializeOwned>(&self) -> Result<T, JsonError> {
        Ok(T::deserialize(&self.0)?)
    }
}

/// As compact JSON text.
impl fmt::Display for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests;
