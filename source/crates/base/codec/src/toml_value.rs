use serde::de::{DeserializeOwned, DeserializeSeed};
use serde::{Deserialize, Serialize};

use crate::error::TomlError;

/// A TOML value read and kept whole, to read as a type later: a part of a data file whose type a
/// registry picks by the key it sits at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TomlValue(pub(crate) toml::Value);

impl TomlValue {
    /// `value` as a TOML value, to place in a table that a writer builds. A type TOML cannot
    /// hold, as a bare `None`, is a caller's error, and panics.
    pub fn of<T: Serialize + ?Sized>(value: &T) -> Self {
        Self(toml::Value::try_from(value).expect("a caller writes only a value TOML holds"))
    }

    /// The value as a `T`.
    pub fn read<T: DeserializeOwned>(self) -> Result<T, TomlError> {
        Ok(T::deserialize(self.0)?)
    }

    /// The value as `seed` reads it, as a type whose read a registry picks.
    pub fn read_with<'de, S: DeserializeSeed<'de>>(self, seed: S) -> Result<S::Value, TomlError> {
        Ok(seed.deserialize(self.0)?)
    }
}
