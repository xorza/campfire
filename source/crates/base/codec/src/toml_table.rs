use serde::Deserialize;
use serde::de::{DeserializeOwned, DeserializeSeed};

use crate::error::TomlError;
use crate::toml_value::TomlValue;

/// A TOML table read and kept whole, to take keys out of and read the rest as a type: as a data
/// file whose parts several types read, each from the keys that are its own.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
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
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::toml::Toml;

    #[test]
    fn a_table_gives_its_keys_out_and_reads_the_rest_as_a_type() {
        let mut table: TomlTable = Toml::parse("cell = 10\nstep = 5\nname = \"grid\"").unwrap();
        assert!(table.holds("step") && !table.holds("none"));
        assert_eq!(table.keys().collect::<Vec<_>>(), ["cell", "name", "step"]);
        assert_eq!(table.take::<String>("name").unwrap().unwrap(), "grid");
        assert!(table.take::<String>("name").is_none());
        assert!(table.clone().take_value("step").is_some());
        assert!(table.take_value("name").is_none());
        assert!(table.take::<String>("cell").unwrap().is_err());
        // `cell` went with its failed read; `step` and the absent `none` split off.
        let kept: Vec<_> = table.clone().into_entries().collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].0, "step");
        assert_eq!(kept[0].1.clone().read::<i64>().unwrap(), 5);
        let split = table.take_keys(["step", "none"]);
        assert_eq!(
            TomlValue::from(split.clone()).read::<BTreeMap<String, i64>>(),
            split.clone().read::<BTreeMap<String, i64>>()
        );
        assert_eq!(
            split.read::<BTreeMap<String, i64>>().unwrap(),
            BTreeMap::from([("step".to_owned(), 5)])
        );
        assert_eq!(
            table.read::<BTreeMap<String, i64>>().unwrap(),
            BTreeMap::new()
        );
    }
}
