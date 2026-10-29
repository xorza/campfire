use serde::de::{DeserializeOwned, DeserializeSeed};
use serde::{Deserialize, Serialize};

use crate::error::{TomlError, TomlWriteError};

/// A TOML value read and kept whole, to read as a type later: a part of a data file whose type a
/// registry picks by the key it sits at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TomlValue(pub(crate) toml::Value);

impl TomlValue {
    /// `value` as a TOML value, to place in a table that a writer builds. A type TOML cannot
    /// hold, as a bare `None`, is a caller's error, and panics.
    pub fn of<T: Serialize + ?Sized>(value: &T) -> Self {
        Self::try_of(value).expect("a caller writes only a value TOML holds")
    }

    /// `value` as a TOML value, or why TOML holds none of it: a value from outside the code, as a
    /// script gives one.
    pub fn try_of<T: Serialize + ?Sized>(value: &T) -> Result<Self, TomlWriteError> {
        Ok(Self(toml::Value::try_from(value)?))
    }

    /// The value as a `T`.
    /// Every string it holds, at any depth: each value of a table and each item of an array.
    pub fn strings(&self) -> Vec<&str> {
        let mut strings = Vec::new();
        Self::strings_of(&self.0, &mut strings);
        strings
    }

    fn strings_of<'a>(value: &'a toml::Value, into: &mut Vec<&'a str>) {
        match value {
            toml::Value::String(text) => into.push(text),
            toml::Value::Array(items) => {
                for item in items {
                    Self::strings_of(item, into);
                }
            }
            toml::Value::Table(table) => {
                for value in table.values() {
                    Self::strings_of(value, into);
                }
            }
            toml::Value::Integer(_)
            | toml::Value::Float(_)
            | toml::Value::Boolean(_)
            | toml::Value::Datetime(_) => {}
        }
    }

    pub fn read<T: DeserializeOwned>(self) -> Result<T, TomlError> {
        Ok(T::deserialize(self.0)?)
    }

    /// The value as `seed` reads it, as a type whose read a registry picks.
    pub fn read_with<'de, S: DeserializeSeed<'de>>(self, seed: S) -> Result<S::Value, TomlError> {
        Ok(seed.deserialize(self.0)?)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn a_value_gives_every_string_it_holds_at_any_depth() {
        // A table of a string, a number, and an array of a string and a table of one.
        #[derive(Serialize)]
        struct Held<'a> {
            a: &'a str,
            n: i64,
            list: (&'a str, BTreeMap<&'a str, &'a str>),
        }
        let inner = BTreeMap::from([("c", "deep")]);
        let value = TomlValue::of(&Held {
            a: "top",
            n: 3,
            list: ("item", inner),
        });
        assert_eq!(value.strings(), ["top", "item", "deep"]);
    }
}
