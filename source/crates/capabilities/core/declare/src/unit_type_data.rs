use std::borrow::Cow;

use serde::Deserializer;
use serde::de::DeserializeSeed;

use crate::section::{Section, SectionTable};
use crate::section_readers::{SectionReaders, TableRead};
use crate::sections::Sections;

/// A unit type as its data file gives it: each key a section of the module that reads it, and
/// whether a section makes it a delivery type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitTypeData {
    sections: Sections,
    delivers: bool,
}

impl UnitTypeData {
    /// What reads a unit type's table, each key as the section `readers` register by it; an
    /// error for a key no module registers, or a value that is no value of its section.
    pub const fn seed(readers: &SectionReaders) -> UnitTypeSeed<'_> {
        UnitTypeSeed(readers)
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

/// What reads a unit type's table, as serde's seed.
#[derive(Debug, Clone, Copy)]
pub struct UnitTypeSeed<'r>(&'r SectionReaders);

impl<'de> DeserializeSeed<'de> for UnitTypeSeed<'_> {
    type Value = UnitTypeData;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<UnitTypeData, D::Error> {
        let table = self.0.table(SectionTable::UnitType);
        let TableRead {
            sections,
            delivers,
            rest,
        } = table.deserialize(deserializer)?;
        if let Some(key) = rest.keys().next() {
            return Err(table.unknown(key, &[]));
        }
        Ok(UnitTypeData { sections, delivers })
    }
}
