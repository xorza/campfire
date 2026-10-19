use std::borrow::Cow;

use campfire_codec::TomlTable;
use serde::de::{DeserializeSeed, Error};
use serde::{Deserialize, Deserializer};

use crate::section::section_readers::{SectionReaders, TableRead};
use crate::section::sections::Sections;
use crate::section::{Section, SectionTable};
use crate::unit_defaults::UnitDefaults;

/// A unit type as its data file gives it: each key a section of the module that reads it, and
/// whether a section makes it a delivery type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitTypeData {
    sections: Sections,
    delivers: bool,
}

impl UnitTypeData {
    /// What reads a unit type's table, each key as the section `readers` register by it, once
    /// `defaults` lie under its sections; an error for a key no module registers, or a value that
    /// is no value of its section.
    pub const fn seed<'r>(
        readers: &'r SectionReaders,
        defaults: &'r UnitDefaults,
    ) -> UnitTypeSeed<'r> {
        UnitTypeSeed { readers, defaults }
    }

    /// Its section `S`, if it gives it.
    pub fn section<S: Section>(&self) -> Option<&S> {
        debug_assert_eq!(S::TABLE.read_in(), SectionTable::UnitType, "{}", S::KEY);
        self.sections.get()
    }

    /// Its section `S`, or the value a type that does not give it reads as.
    pub fn section_or_default<S: Section + Default>(&self) -> Cow<'_, S> {
        debug_assert_eq!(S::TABLE.read_in(), SectionTable::UnitType, "{}", S::KEY);
        self.sections.get_or_default()
    }

    pub const fn sections(&self) -> &Sections {
        &self.sections
    }

    /// Whether it is a delivery type: a section of `SectionTable::DeliveryType` makes it one,
    /// which no map or train places.
    pub const fn delivers(&self) -> bool {
        self.delivers
    }
}

/// What reads a unit type's table, as serde's seed: the mode's defaults lie under the sections it
/// gives first.
#[derive(Debug, Clone, Copy)]
pub struct UnitTypeSeed<'r> {
    readers: &'r SectionReaders,
    defaults: &'r UnitDefaults,
}

impl<'de> DeserializeSeed<'de> for UnitTypeSeed<'_> {
    type Value = UnitTypeData;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<UnitTypeData, D::Error> {
        let mut given = TomlTable::deserialize(deserializer)?;
        self.defaults.lay_under(&mut given);
        let table = self.readers.table(SectionTable::UnitType);
        let TableRead {
            sections,
            delivers,
            rest,
        } = given.read_with(table).map_err(D::Error::custom)?;
        if let Some(key) = rest.keys().next() {
            return Err(table.unknown(key, &[]));
        }
        Ok(UnitTypeData { sections, delivers })
    }
}
