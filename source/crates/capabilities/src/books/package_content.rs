use std::collections::BTreeMap;
use std::fmt;

use campfire_actions::ActionData;
use campfire_common::DeclaredName;
use campfire_declare::{SectionReaders, UnitTypeData};
use campfire_items::ItemData;
use campfire_stats::ModifierData;
use serde::Deserializer;
use serde::de::{DeserializeSeed, Error, MapAccess, Visitor};

use crate::books::units_seed::UnitsSeed;

/// What a package holds for a match, each by its id in the package: its actions, its modifiers,
/// its unit types and its item types. The mode, an avatar and a loadout hold the same shape, and
/// the load lets only the mode hold item types.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageContent {
    pub actions: BTreeMap<DeclaredName, ActionData>,
    pub modifiers: BTreeMap<DeclaredName, ModifierData>,
    pub units: BTreeMap<DeclaredName, UnitTypeData>,
    pub items: BTreeMap<DeclaredName, ItemData>,
}

impl PackageContent {
    const ACTIONS: &'static str = "actions";
    const MODIFIERS: &'static str = "modifiers";
    const UNITS: &'static str = "units";
    const ITEMS: &'static str = "items";

    /// The keys of a data file's table that hold a package's content.
    pub const KEYS: [&'static str; 4] = [Self::ACTIONS, Self::MODIFIERS, Self::UNITS, Self::ITEMS];

    /// What reads a package's content, its unit types' sections as `readers` register them.
    pub const fn seed(readers: &SectionReaders) -> ContentSeed<'_> {
        ContentSeed(readers)
    }
}

/// What reads a package's content, as serde's seed.
#[derive(Debug, Clone, Copy)]
pub struct ContentSeed<'r>(&'r SectionReaders);

impl<'de> DeserializeSeed<'de> for ContentSeed<'_> {
    type Value = PackageContent;

    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<PackageContent, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for ContentSeed<'_> {
    type Value = PackageContent;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a package's content")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<PackageContent, A::Error> {
        let mut content = PackageContent::default();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                PackageContent::ACTIONS => content.actions = map.next_value()?,
                PackageContent::MODIFIERS => content.modifiers = map.next_value()?,
                PackageContent::UNITS => content.units = map.next_value_seed(UnitsSeed(self.0))?,
                PackageContent::ITEMS => content.items = map.next_value()?,
                _ => return Err(A::Error::unknown_field(&key, &PackageContent::KEYS)),
            }
        }
        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::{Toml, TomlTable};

    use super::*;
    use crate::release::Release;

    #[test]
    fn a_package_reads_its_four_keys_and_no_other() {
        let read = |text: &str| {
            let raw = Toml::parse::<TomlTable>(text).unwrap();
            raw.read_with(PackageContent::seed(&Release::sections()))
        };
        let content =
            read("[units.scout]\ntags = [\"fast\"]\n[actions]\n[modifiers]\n[items]").unwrap();
        assert_eq!(
            content
                .units
                .keys()
                .map(DeclaredName::as_str)
                .collect::<Vec<_>>(),
            ["scout"]
        );
        assert_eq!(
            read("[unit]").unwrap_err().message(),
            "unknown field `unit`, expected one of `actions`, `modifiers`, `units`, `items`"
        );
    }
}
