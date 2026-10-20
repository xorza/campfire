use std::collections::BTreeMap;
use std::fmt;

use campfire_common::DeclaredName;
use campfire_declarations::{SectionReaders, UnitDefaults, UnitTypeData};
use serde::Deserializer;
use serde::de::{DeserializeSeed, MapAccess, Visitor};

/// What reads a table of unit types by name, `[units.<name>]`, each as the sections of its
/// readers read it, once the mode's defaults lie under them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnitsSeed<'r> {
    pub(crate) readers: &'r SectionReaders,
    pub(crate) defaults: &'r UnitDefaults,
}

impl<'de> DeserializeSeed<'de> for UnitsSeed<'_> {
    type Value = BTreeMap<DeclaredName, UnitTypeData>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for UnitsSeed<'_> {
    type Value = BTreeMap<DeclaredName, UnitTypeData>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a table of unit types by name")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut units = BTreeMap::new();
        while let Some(name) = map.next_key::<DeclaredName>()? {
            let unit = map.next_value_seed(UnitTypeData::seed(self.readers, self.defaults))?;
            units.insert(name, unit);
        }
        Ok(units)
    }
}
