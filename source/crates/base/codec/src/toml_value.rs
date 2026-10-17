use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::TomlError;

/// A TOML value read and kept whole, to read as a type later: a part of a data file whose type a
/// registry picks by the key it sits at.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(transparent)]
pub struct TomlValue(pub(crate) toml::Value);

impl TomlValue {
    /// The value as a `T`.
    pub fn read<T: DeserializeOwned>(self) -> Result<T, TomlError> {
        Ok(T::deserialize(self.0)?)
    }
}
