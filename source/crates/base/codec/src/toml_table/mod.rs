use serde::de::{DeserializeOwned, DeserializeSeed};
use serde::{Deserialize, Serialize};

use crate::error::TomlError;
use crate::toml_value::TomlValue;

/// A TOML table read and kept whole, to take keys out of and read the rest as a type: as a data
/// file whose parts several types read, each from the keys that are its own.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TomlTable(toml::Table);

impl TomlTable {
    /// Whether it holds `key`.
    pub fn holds(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// Its keys, in order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    /// The value of `key`, taken out of it and read as a `T`; none when it holds no `key`.
    pub fn take<T: DeserializeOwned>(&mut self, key: &str) -> Option<Result<T, TomlError>> {
        let value = self.0.remove(key)?;
        Some(T::deserialize(value).map_err(TomlError::from))
    }

    /// The value of `key`, taken out of it and kept whole; none when it holds no `key`.
    pub fn take_value(&mut self, key: &str) -> Option<TomlValue> {
        self.0.remove(key).map(TomlValue)
    }

    /// The values of `keys` it holds, taken out of it into a table of their own.
    #[must_use]
    pub fn take_keys<'k>(&mut self, keys: impl IntoIterator<Item = &'k str>) -> Self {
        Self(
            keys.into_iter()
                .filter_map(|key| self.0.remove_entry(key))
                .collect(),
        )
    }

    /// Its keys and their values kept whole, in the order of the keys.
    pub fn into_entries(self) -> impl ExactSizeIterator<Item = (String, TomlValue)> {
        self.0
            .into_iter()
            .map(|(key, value)| (key, TomlValue(value)))
    }

    /// Puts `value` at `key`, in place of the value it holds there.
    pub fn insert(&mut self, key: String, value: TomlValue) {
        self.0.insert(key, value.0);
    }

    /// Lays it over `base`, as a later package's data over an earlier one's
    /// (03-game-scripting.md, Overrides by load order): each key of `base` it lacks joins it, and
    /// of a key both hold its own value stays whole, but where both are tables, whose keys merge
    /// so, one level down. Gives the keys, and the `key.inner` of the tables, that both held, in
    /// order.
    pub fn lay_over(&mut self, base: Self) -> Vec<String> {
        let mut overridden = Vec::new();
        for (key, value) in base.0 {
            let Some(own) = self.0.get_mut(&key) else {
                self.0.insert(key, value);
                continue;
            };
            match (own, value) {
                (toml::Value::Table(own), toml::Value::Table(base)) => {
                    for (entry, value) in base {
                        if own.contains_key(&entry) {
                            overridden.push(format!("{key}.{entry}"));
                        } else {
                            own.insert(entry, value);
                        }
                    }
                }
                _ => overridden.push(key),
            }
        }
        overridden.sort();
        overridden
    }

    /// Fills each table it holds at a key where `base` holds a table too with the entries of
    /// `base`'s it lacks, one level down: its own values stay, and a key it does not hold stays
    /// absent, as a unit type's sections take a mode's defaults.
    pub fn fill_held(&mut self, base: &Self) {
        for (key, value) in &base.0 {
            let (Some(toml::Value::Table(own)), toml::Value::Table(base)) =
                (self.0.get_mut(key), value)
            else {
                continue;
            };
            for (entry, value) in base {
                if !own.contains_key(entry) {
                    own.insert(entry.clone(), value.clone());
                }
            }
        }
    }

    /// The table as a `T`.
    pub fn read<T: DeserializeOwned>(self) -> Result<T, TomlError> {
        Ok(T::deserialize(toml::Value::Table(self.0))?)
    }

    /// The table as `seed` reads it, as a type whose read a registry picks.
    pub fn read_with<'de, S: DeserializeSeed<'de>>(self, seed: S) -> Result<S::Value, TomlError> {
        Ok(seed.deserialize(toml::Value::Table(self.0))?)
    }
}

/// The table as a value, as a type a registry picks reads the rest of a table.
impl From<TomlTable> for TomlValue {
    fn from(table: TomlTable) -> Self {
        Self(toml::Value::Table(table.0))
    }
}

#[cfg(test)]
mod tests;
