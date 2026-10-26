use std::collections::BTreeMap;
use std::{fmt, iter};

use campfire_actions::{ActionData, ActionsData};
use campfire_common::DeclaredName;
use campfire_declarations::{
    Section, SectionReaders, SectionTable, Sections, TableRead, UnitDefaults, UnitTypeData,
};
use campfire_items::{ItemData, ItemsData};
use campfire_stats::{ModifierData, ModifiersData};
use serde::Deserializer;
use serde::de::{DeserializeSeed, MapAccess, Visitor};

use crate::books::units_seed::UnitsSeed;

/// What a package holds for a match: its unit types by name, and the sections of the modules that
/// read the rest, its actions, modifiers and item types by id. The mode, an avatar and a loadout
/// hold the same shape, and the load lets only the mode hold item types.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageContent {
    pub units: BTreeMap<DeclaredName, UnitTypeData>,
    pub sections: Sections,
}

impl PackageContent {
    const UNITS: &'static str = "units";

    /// The keys of a data file's table that hold a package's content, as `readers` register its
    /// sections.
    pub fn keys(readers: &SectionReaders) -> impl Iterator<Item = &'static str> {
        iter::once(Self::UNITS).chain(readers.keys(SectionTable::Package))
    }

    /// What reads a package's content, its sections and its unit types' as `readers` register
    /// them, `defaults` under its unit types' sections.
    pub const fn seed<'r>(
        readers: &'r SectionReaders,
        defaults: &'r UnitDefaults,
    ) -> ContentSeed<'r> {
        ContentSeed { readers, defaults }
    }

    /// Its section `S`, if it gives it.
    pub fn section<S: Section>(&self) -> Option<&S> {
        debug_assert_eq!(S::TABLE, SectionTable::Package, "{}", S::KEY);
        self.sections.get()
    }

    /// Its actions, by id: none when it gives no `[actions]`.
    pub fn actions(&self) -> &BTreeMap<DeclaredName, ActionData> {
        static NONE: ActionsData = ActionsData(BTreeMap::new());
        &self.section().unwrap_or(&NONE).0
    }

    /// Its modifiers, by id: none when it gives no `[modifiers]`.
    pub fn modifiers(&self) -> &BTreeMap<DeclaredName, ModifierData> {
        static NONE: ModifiersData = ModifiersData(BTreeMap::new());
        &self.section().unwrap_or(&NONE).0
    }

    /// Its item types, by id: none when it gives no `[items]`.
    pub fn items(&self) -> &BTreeMap<DeclaredName, ItemData> {
        static NONE: ItemsData = ItemsData(BTreeMap::new());
        &self.section().unwrap_or(&NONE).0
    }
}

/// What reads a package's content, as serde's seed.
#[derive(Debug, Clone, Copy)]
pub struct ContentSeed<'r> {
    readers: &'r SectionReaders,
    defaults: &'r UnitDefaults,
}

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
        let readers = self.readers;
        let table = readers.table(SectionTable::Package);
        let mut read = TableRead::default();
        let mut units = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            if key == PackageContent::UNITS {
                units = map.next_value_seed(UnitsSeed {
                    readers,
                    defaults: self.defaults,
                })?;
            } else if !table.next_section(&key, &mut map, &mut read)? {
                return Err(table.unknown(&key, &[PackageContent::UNITS]));
            }
        }
        Ok(PackageContent {
            units,
            sections: read.sections,
        })
    }
}

#[cfg(test)]
mod tests;
