use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::TomlError;

/// A TOML table read and kept whole, to take keys out of and read the rest as a type: as a data
/// file whose parts several types read, each from the keys that are its own.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(transparent)]
pub struct TomlTable(toml::Table);

impl TomlTable {
    /// Whether it holds `key`.
    pub fn holds(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// The value of `key`, taken out of it and read as a `T`; none when it holds no `key`.
    pub fn take<T: DeserializeOwned>(&mut self, key: &str) -> Option<Result<T, TomlError>> {
        let value = self.0.remove(key)?;
        Some(T::deserialize(value).map_err(TomlError::from))
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

    /// The table as a `T`.
    pub fn read<T: DeserializeOwned>(self) -> Result<T, TomlError> {
        Ok(T::deserialize(toml::Value::Table(self.0))?)
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
        assert_eq!(table.take::<String>("name").unwrap().unwrap(), "grid");
        assert!(table.take::<String>("name").is_none());
        assert!(table.take::<String>("cell").unwrap().is_err());
        // `cell` went with its failed read; `step` and the absent `none` split off.
        let split = table.take_keys(["step", "none"]);
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
