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
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::toml::Toml;

    #[test]
    fn a_table_fills_only_the_tables_it_holds_with_the_entries_it_lacks() {
        let mut unit: TomlTable =
            Toml::parse("combat = { armor = \"plate\" }\nturret = {}\ntags = [\"a\"]").unwrap();
        let base: TomlTable = Toml::parse(
            "combat = { armor = \"cloth\", on_death = \"stay\" }\nturret = { tolerance_deg = 5 \
             }\nvision = { sight_range = \"6\" }\ntags = { x = 1 }",
        )
        .unwrap();
        unit.fill_held(&base);
        // Its own `armor` stays and `on_death` joins; the empty `turret` takes its default; it
        // gives no `vision`, which stays absent; `tags` is no table, which stays as it was.
        let expected: TomlTable = Toml::parse(
            "combat = { armor = \"plate\", on_death = \"stay\" }\nturret = { tolerance_deg = 5 \
             }\ntags = [\"a\"]",
        )
        .unwrap();
        assert_eq!(unit, expected);
    }

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

    #[test]
    fn a_table_laid_over_another_keeps_its_own_values_and_merges_its_tables_keys() {
        let mut mode: TomlTable =
            Toml::parse("name = \"mode\"\n[combat]\nlife = \"hp\"\n[state]\nphase = 1\nround = 2")
                .unwrap();
        let rules: TomlTable = Toml::parse(
            "name = \"rules\"\ntimers = [\"a\"]\n[combat]\nkinds = [\"x\"]\n[state]\nphase = 9\nscore = 3",
        )
        .unwrap();
        // `name` stays the mode's, `timers` joins it, and the tables' keys merge: `[combat]` takes
        // the rules' `kinds` beside its own `life`, and the state the rules' `score`, its own
        // `phase` over the rules'.
        let overridden = mode.lay_over(rules);
        assert_eq!(overridden, ["name", "state.phase"]);
        assert_eq!(
            mode.clone().take::<String>("name").unwrap().unwrap(),
            "mode"
        );
        let combat: BTreeMap<String, toml::Value> = mode.clone().take("combat").unwrap().unwrap();
        assert_eq!(combat.keys().collect::<Vec<_>>(), ["kinds", "life"]);
        assert_eq!(combat["life"].as_str(), Some("hp"));
        let state: BTreeMap<String, i64> = mode.clone().take("state").unwrap().unwrap();
        let expected = [("phase", 1), ("round", 2), ("score", 3)];
        assert_eq!(
            state,
            expected.map(|(key, value)| (key.to_owned(), value)).into()
        );
        assert_eq!(mode.take::<Vec<String>>("timers").unwrap().unwrap(), ["a"]);
        // A key that is a table on one side only stays the top's whole, and a table two levels
        // down is one key's value, whole.
        let mut flat: TomlTable = Toml::parse("state = 1").unwrap();
        let tabled: TomlTable = Toml::parse("[state]\nphase = 1").unwrap();
        assert_eq!(flat.lay_over(tabled), ["state"]);
        assert_eq!(flat.take::<i64>("state").unwrap().unwrap(), 1);
        let mut deep: TomlTable = Toml::parse("[combat.armors.tank]\nshell = 1").unwrap();
        let under: TomlTable = Toml::parse("[combat.armors.jeep]\nshell = 2").unwrap();
        assert_eq!(deep.lay_over(under), ["combat.armors"]);
        let combat: BTreeMap<String, toml::Value> = deep.take("combat").unwrap().unwrap();
        let armors = combat["armors"].as_table().unwrap();
        assert_eq!(armors.keys().collect::<Vec<_>>(), ["tank"]);
    }
}
