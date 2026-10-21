use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::TomlError;

/// TOML, the format of a package's data files and the configuration a person writes.
#[derive(Debug)]
pub struct Toml;

impl Toml {
    /// The value `text` holds.
    pub fn parse<T: DeserializeOwned>(text: &str) -> Result<T, TomlError> {
        Ok(toml::from_str(text)?)
    }

    /// `value` as a TOML document. A type TOML cannot hold, as one of no table at its top, is a
    /// caller's error, and panics.
    pub fn write<T: Serialize + ?Sized>(value: &T) -> String {
        toml::to_string(value).expect("a caller writes only a type TOML holds")
    }
}

#[cfg(test)]
mod tests;
