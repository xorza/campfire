use std::any::{Any, TypeId};
use std::fmt;
use std::sync::Arc;

use campfire_codec::{TomlError, TomlTable, TomlValue};
use campfire_sim::Capability;
use serde::de::{DeserializeOwned, DeserializeSeed, Error, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::section::sections::{SectionValue, Sections};
use crate::section::{Listed, ListedSet, Section, SectionTable};

/// The sections every module of the release registers, each by its table and key, and the
/// members of each open set, each by its key, in the order of the module list: what reads a data
/// table's sections, and the members a section holds.
#[derive(Debug, Default)]
pub struct SectionReaders {
    sections: Vec<SectionReader>,
    listed: Vec<ListedReader>,
}

/// How one section reads: its table, its key, the capability of the module that registers it,
/// none for the core's, and its read of the value at its key.
#[derive(Debug, Clone, Copy)]
struct SectionReader {
    table: SectionTable,
    key: &'static str,
    owner: Option<Capability>,
    read: fn(TomlValue, &SectionReaders) -> Result<Box<dyn SectionValue>, TomlError>,
}

/// How a member of an open set reads: its value as its set holds it.
pub type ListedRead<S> =
    fn(TomlValue, &SectionReaders) -> Result<Arc<<S as ListedSet>::Value>, TomlError>;

/// How one member of an open set reads: its set, its key, and its `ListedRead` of that set.
#[derive(Debug)]
struct ListedReader {
    set: TypeId,
    key: &'static str,
    read: Box<dyn Any + Send + Sync>,
}

impl SectionReaders {
    /// Registers `S`, of a module of `owner`, none for the core's; a key that a section of its
    /// table registers already is a logic error.
    pub fn add<S: Section>(&mut self, owner: Option<Capability>) {
        let table = S::TABLE.read_in();
        assert!(
            self.reader(table, S::KEY).is_none(),
            "two sections of one table take the key {}",
            S::KEY
        );
        self.sections.push(SectionReader {
            table: S::TABLE,
            key: S::KEY,
            owner,
            read: |value, readers| {
                let section: Box<dyn SectionValue> = Box::new(S::read_value(value, readers)?);
                Ok(section)
            },
        });
    }

    /// Registers `L`, a member of its set; a key that another member of the set takes already is
    /// a logic error.
    pub fn add_listed<L: Listed>(&mut self) {
        assert!(
            self.listed::<L::Set>(L::KEY).is_none(),
            "two members of one set take the key {}",
            L::KEY
        );
        let read: ListedRead<L::Set> =
            |value, readers| Ok(L::read_value(value, readers)?.into_listed());
        self.listed.push(ListedReader {
            set: TypeId::of::<L::Set>(),
            key: L::KEY,
            read: Box::new(read),
        });
    }

    /// The read of the member of `S` at `key`; none for a key no member takes.
    pub fn listed<S: ListedSet>(&self, key: &str) -> Option<ListedRead<S>> {
        let reader = self.members::<S>().find(|reader| reader.key == key)?;
        let read = reader.read.downcast_ref::<ListedRead<S>>();
        Some(*read.expect("a set's member reads as its set's value"))
    }

    /// The keys of the members of `S`, in the order the modules register them.
    pub fn listed_keys<S: ListedSet>(&self) -> impl Iterator<Item = &'static str> {
        self.members::<S>().map(|reader| reader.key)
    }

    fn members<S: ListedSet>(&self) -> impl Iterator<Item = &ListedReader> {
        let set = TypeId::of::<S>();
        self.listed.iter().filter(move |reader| reader.set == set)
    }

    /// What reads a table of `table`: its sections, and the rest of its keys kept whole.
    pub const fn table(&self, table: SectionTable) -> TableSeed<'_> {
        TableSeed {
            readers: self,
            table,
        }
    }

    /// The keys of `table`'s sections, in the order the modules register them.
    pub fn keys(&self, table: SectionTable) -> impl Iterator<Item = &'static str> {
        self.of(table).map(|reader| reader.key)
    }

    /// The keys of the delivery types' sections, a part of the unit types'.
    pub fn delivery_keys(&self) -> impl Iterator<Item = &'static str> {
        let delivers = self.sections.iter();
        let delivers = delivers.filter(|reader| reader.table == SectionTable::DeliveryType);
        delivers.map(|reader| reader.key)
    }

    /// The capability of the module that registers the section `key` of `table`; none for a
    /// section of the core's, or of no module.
    pub fn owner(&self, table: SectionTable, key: &str) -> Option<Capability> {
        self.reader(table.read_in(), key)?.owner
    }

    fn of(&self, table: SectionTable) -> impl Iterator<Item = &SectionReader> {
        self.sections
            .iter()
            .filter(move |reader| reader.table.read_in() == table)
    }

    fn reader(&self, table: SectionTable, key: &str) -> Option<&SectionReader> {
        self.of(table).find(|reader| reader.key == key)
    }
}

/// What reads a data table of sections, as serde's seed: each key of a section of its table as
/// that section's type, and the rest kept whole for the table's own fields. The format's reader
/// walks the keys, so an error names the key it is at.
#[derive(Debug, Clone, Copy)]
pub struct TableSeed<'r> {
    readers: &'r SectionReaders,
    table: SectionTable,
}

/// A data table as its seed reads it: its sections, whether one makes its unit type a delivery
/// type, and the rest of its keys kept whole, which its own fields read.
#[derive(Debug, Default)]
pub struct TableRead {
    pub sections: Sections,
    pub delivers: bool,
    pub rest: TomlTable,
}

impl TableRead {
    /// Its own fields, of the rest of its keys, as a `T`; an error for a key `T` does not read,
    /// in the error type of the read it is part of.
    pub fn own<T: DeserializeOwned, E: Error>(rest: TomlTable) -> Result<T, E> {
        rest.read().map_err(TomlError::into_de)
    }
}

impl TableSeed<'_> {
    /// The error of `key`, which neither the table's own keys `own` nor its sections take.
    pub fn unknown<E: Error>(&self, key: &str, own: &[&'static str]) -> E {
        let keys = own.iter().copied().chain(self.readers.keys(self.table));
        let keys: Vec<String> = keys.map(|key| format!("`{key}`")).collect();
        E::custom(format_args!(
            "unknown field `{key}`, expected one of {}",
            keys.join(", ")
        ))
    }

    /// Reads the value at `key` of `map` into `read` as its section, when a section of the table
    /// takes `key`; whether one does. A table that holds keys of its own reads each of them
    /// itself, as a package its unit types.
    pub fn next_section<'de, A: MapAccess<'de>>(
        &self,
        key: &str,
        map: &mut A,
        read: &mut TableRead,
    ) -> Result<bool, A::Error> {
        let Some(reader) = self.readers.reader(self.table, key) else {
            return Ok(false);
        };
        let seed = SectionSeed {
            reader,
            readers: self.readers,
        };
        read.sections.push(map.next_value_seed(seed)?);
        read.delivers |= reader.table == SectionTable::DeliveryType;
        Ok(true)
    }
}

impl<'de> DeserializeSeed<'de> for TableSeed<'_> {
    type Value = TableRead;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<TableRead, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for TableSeed<'_> {
    type Value = TableRead;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a table")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<TableRead, A::Error> {
        let mut read = TableRead {
            sections: Sections::default(),
            delivers: false,
            rest: TomlTable::default(),
        };
        while let Some(key) = map.next_key::<String>()? {
            if let Some(reader) = self.readers.reader(self.table, &key) {
                let seed = SectionSeed {
                    reader,
                    readers: self.readers,
                };
                read.sections.push(map.next_value_seed(seed)?);
                read.delivers |= reader.table == SectionTable::DeliveryType;
            } else {
                read.rest.insert(key, map.next_value()?);
            }
        }
        Ok(read)
    }
}

/// What reads one section's value as its type, with the release's readers of the open sets it
/// holds, its error the section's own, which the format's reader places at the section's key.
#[derive(Debug)]
struct SectionSeed<'r> {
    reader: &'r SectionReader,
    readers: &'r SectionReaders,
}

impl<'de> DeserializeSeed<'de> for SectionSeed<'_> {
    type Value = Box<dyn SectionValue>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let value = TomlValue::deserialize(deserializer)?;
        (self.reader.read)(value, self.readers).map_err(TomlError::into_de)
    }
}

#[cfg(test)]
mod tests;
