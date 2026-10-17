use std::fmt;

use campfire_codec::{TomlError, TomlTable, TomlValue};
use serde::de::{DeserializeOwned, DeserializeSeed, Error, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::section::{Section, SectionTable};
use crate::sections::{SectionValue, Sections};

/// The sections every module of the release registers, each by its table and key, in the order
/// of the module list: what reads a data table's sections.
#[derive(Debug, Default)]
pub struct SectionReaders(Vec<SectionReader>);

/// How one section reads: its table, its key, and its read of the value at its key.
#[derive(Debug, Clone, Copy)]
struct SectionReader {
    table: SectionTable,
    key: &'static str,
    read: fn(TomlValue) -> Result<Box<dyn SectionValue>, TomlError>,
}

impl SectionReaders {
    /// Registers `S`; a key that a section of its table registers already is a logic error.
    pub fn add<S: Section>(&mut self) {
        let table = S::TABLE.read_in();
        assert!(
            self.reader(table, S::KEY).is_none(),
            "two sections of one table take the key {}",
            S::KEY
        );
        self.0.push(SectionReader {
            table: S::TABLE,
            key: S::KEY,
            read: |value| {
                let section: Box<dyn SectionValue> = Box::new(value.read::<S>()?);
                Ok(section)
            },
        });
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
        let delivers = self.0.iter();
        let delivers = delivers.filter(|reader| reader.table == SectionTable::DeliveryType);
        delivers.map(|reader| reader.key)
    }

    fn of(&self, table: SectionTable) -> impl Iterator<Item = &SectionReader> {
        self.0
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
        read.sections
            .push(map.next_value_seed(SectionSeed(reader))?);
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
                read.sections
                    .push(map.next_value_seed(SectionSeed(reader))?);
                read.delivers |= reader.table == SectionTable::DeliveryType;
            } else {
                read.rest.insert(key, map.next_value()?);
            }
        }
        Ok(read)
    }
}

/// What reads one section's value as its type, its error the section's own, which the format's
/// reader places at the section's key.
#[derive(Debug)]
struct SectionSeed<'r>(&'r SectionReader);

impl<'de> DeserializeSeed<'de> for SectionSeed<'_> {
    type Value = Box<dyn SectionValue>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let value = TomlValue::deserialize(deserializer)?;
        (self.0.read)(value).map_err(TomlError::into_de)
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use campfire_codec::Toml;

    use super::*;
    use crate::unit_type_data::UnitTypeData;

    /// A unit type's section of a test.
    #[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Speed {
        value: i64,
    }

    impl Section for Speed {
        const TABLE: SectionTable = SectionTable::UnitType;
        const KEY: &'static str = "speed";
    }

    /// A delivery type's section of a test.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Bolt {
        reach: i64,
    }

    impl Section for Bolt {
        const TABLE: SectionTable = SectionTable::DeliveryType;
        const KEY: &'static str = "bolt";
    }

    /// A map's section of a test.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    struct Grid {
        cell: i64,
    }

    impl Section for Grid {
        const TABLE: SectionTable = SectionTable::Map;
        const KEY: &'static str = "grid";
    }

    /// A unit type's section of a test that takes `Speed`'s key.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    struct OtherSpeed;

    impl Section for OtherSpeed {
        const TABLE: SectionTable = SectionTable::UnitType;
        const KEY: &'static str = "speed";
    }

    fn readers() -> SectionReaders {
        let mut readers = SectionReaders::default();
        readers.add::<Speed>();
        readers.add::<Bolt>();
        readers.add::<Grid>();
        readers
    }

    fn table(text: &str, table: SectionTable) -> TableRead {
        let raw = Toml::parse::<TomlTable>(text).unwrap();
        raw.read_with(readers().table(table)).unwrap()
    }

    #[test]
    fn a_table_reads_the_sections_of_its_own_kind_and_keeps_the_rest() {
        // A unit type's table reads its `speed` and the delivery type's `bolt`, which makes it
        // a delivery type, and keeps `name`, its own field.
        let text = "speed = { value = 3 }\nbolt = { reach = 2 }\nname = \"x\"";
        let read = table(text, SectionTable::UnitType);
        assert_eq!(read.sections.get::<Speed>(), Some(&Speed { value: 3 }));
        assert_eq!(read.sections.get::<Bolt>(), Some(&Bolt { reach: 2 }));
        assert!(read.delivers);
        assert_eq!(read.rest.keys().collect::<Vec<_>>(), ["name"]);
        // Without `bolt` it is no delivery type.
        let read = table("speed = { value = 3 }", SectionTable::UnitType);
        assert_eq!(read.sections.keys().collect::<Vec<_>>(), ["speed"]);
        assert!(!read.delivers);
        // A map's table reads none of a unit type's sections: they are its own fields.
        let read = table(text, SectionTable::Map);
        assert!(read.sections.is_empty());
        assert_eq!(
            read.rest.keys().collect::<Vec<_>>(),
            ["bolt", "name", "speed"]
        );
        // Each table's keys, the unit type's with the delivery type's.
        let readers = readers();
        assert_eq!(
            readers.keys(SectionTable::UnitType).collect::<Vec<_>>(),
            ["speed", "bolt"]
        );
        assert_eq!(readers.delivery_keys().collect::<Vec<_>>(), ["bolt"]);
        assert_eq!(
            readers.keys(SectionTable::Map).collect::<Vec<_>>(),
            ["grid"]
        );
    }

    #[test]
    fn a_unit_type_refuses_a_key_no_section_takes_and_keeps_a_sections_own_message() {
        let read = |text: &str| {
            let raw = Toml::parse::<TomlTable>(text).unwrap();
            raw.read_with(UnitTypeData::seed(&readers()))
        };
        let unit = read("speed = { value = 3 }\nbolt = { reach = 2 }").unwrap();
        assert_eq!(unit.section::<Speed>(), Some(&Speed { value: 3 }));
        assert!(unit.delivers());
        assert_eq!(
            read("speed = { value = 3 }\ncolor = 1")
                .unwrap_err()
                .message(),
            "unknown field `color`, expected one of `speed`, `bolt`"
        );
        // A section's error is its type's, first in the message, its place the format's.
        let wrong = read("speed = { value = \"fast\" }").unwrap_err();
        assert!(
            wrong
                .message()
                .starts_with("invalid type: string \"fast\", expected i64"),
            "{wrong}"
        );
    }

    #[test]
    #[should_panic(expected = "two sections of one table take the key speed")]
    fn two_sections_of_one_table_take_no_key_twice() {
        let mut readers = readers();
        readers.add::<OtherSpeed>();
    }

    #[test]
    fn sections_compare_in_any_order_and_a_section_replaces_its_own_type() {
        let speed = |value| Speed { value };
        let held = Sections::default().with(speed(1)).with(Bolt { reach: 2 });
        let other = Sections::default().with(Bolt { reach: 2 }).with(speed(1));
        assert_eq!(held, other);
        assert_ne!(held, Sections::default().with(speed(1)));
        let replaced = held.clone().with(speed(4));
        assert_eq!(replaced.get::<Speed>(), Some(&speed(4)));
        assert_eq!(replaced.keys().collect::<Vec<_>>(), ["bolt", "speed"]);
        assert_ne!(replaced, held);
        // The default stands for a section the table does not give.
        assert!(matches!(
            held.get_or_default::<Speed>(),
            Cow::Borrowed(&Speed { value: 1 })
        ));
        let none = Sections::default();
        assert!(matches!(
            none.get_or_default::<Speed>(),
            Cow::Owned(Speed { value: 0 })
        ));
    }
}
