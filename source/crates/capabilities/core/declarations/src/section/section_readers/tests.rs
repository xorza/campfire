use std::borrow::Cow;

use campfire_codec::Toml;
use campfire_script::{ApiBuilder, DataTable};
use campfire_script::{ApiVersion, Status};

use super::*;
use crate::section::ReadValue;
use crate::unit_defaults::UnitDefaults;
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
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
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
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

/// A map's section of a test.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Grid {
    cell: i64,
}

impl Section for Grid {
    const TABLE: SectionTable = SectionTable::Map;
    const KEY: &'static str = "grid";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

/// A unit type's section of a test that takes `Speed`'s key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct OtherSpeed;

impl Section for OtherSpeed {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "speed";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

/// An open set of a test, whose members read as their `Debug`.
#[derive(Debug)]
struct Shown;

impl ListedSet for Shown {
    type Value = dyn fmt::Debug + Send + Sync;

    const TABLE: DataTable =
        DataTable::new("An effect of an action's `on_resolve`, `on_hit` or `on_end`");
}

/// A second open set of a test, of the same members.
#[derive(Debug)]
struct Others;

impl ListedSet for Others {
    type Value = dyn fmt::Debug + Send + Sync;

    const TABLE: DataTable =
        DataTable::new("An effect of an action's `on_resolve`, `on_hit` or `on_end`");
}

/// A member of `Shown`, and, as `OtherTwo`, of `Others`, by one key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Two {
    value: i64,
}

impl Listed for Two {
    type Set = Shown;

    const KEY: &'static str = "two";

    const STATUS: Status = Status::Runs(ApiVersion::FIRST);

    fn into_listed(self) -> Arc<dyn fmt::Debug + Send + Sync> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct OtherTwo {
    value: i64,
}

impl Listed for OtherTwo {
    type Set = Others;

    const KEY: &'static str = "two";

    const STATUS: Status = Status::Runs(ApiVersion::FIRST);

    fn into_listed(self) -> Arc<dyn fmt::Debug + Send + Sync> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}

/// A map's section of a test that holds a member of `Shown` at `item`, which it reads with
/// the readers its read is given.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Bag(String);

impl Section for Bag {
    const TABLE: SectionTable = SectionTable::Map;
    const KEY: &'static str = "bag";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

impl ReadValue for Bag {
    fn read_value(value: TomlValue, readers: &SectionReaders) -> Result<Self, TomlError> {
        let mut table = value.read::<TomlTable>()?;
        let item = table.take_value("item").expect("a bag holds an item");
        let read = readers.listed::<Shown>("two").expect("`two` is a member");
        Ok(Self(format!("{:?}", read(item, readers)?)))
    }

    fn fields() -> &'static [&'static str] {
        &["item"]
    }
}

fn readers() -> SectionReaders {
    let mut readers = SectionReaders::default();
    readers.add::<Speed>(None);
    readers.add::<Bolt>(None);
    readers.add::<Grid>(None);
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
    // The fields a default may give: of a unit type's section, a delivery type's among them by the
    // unit type's table, and of a map's in its own table alone.
    assert_eq!(
        readers.fields(SectionTable::UnitType, "speed"),
        Some(&["value"][..])
    );
    assert_eq!(
        readers.fields(SectionTable::UnitType, "bolt"),
        Some(&["reach"][..])
    );
    assert_eq!(readers.fields(SectionTable::UnitType, "grid"), None);
    assert_eq!(
        readers.fields(SectionTable::Map, "grid"),
        Some(&["cell"][..])
    );
    assert_eq!(
        readers.keys(SectionTable::Map).collect::<Vec<_>>(),
        ["grid"]
    );
}

#[test]
fn a_unit_type_refuses_a_key_no_section_takes_and_keeps_a_sections_own_message() {
    let read = |text: &str| {
        let raw = Toml::parse::<TomlTable>(text).unwrap();
        raw.read_with(UnitTypeData::seed(&readers(), &UnitDefaults::default()))
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
    readers.add::<OtherSpeed>(None);
}

#[test]
fn an_open_sets_member_reads_by_its_key_in_its_set_alone_and_a_section_reads_it() {
    let mut readers = readers();
    readers.add_listed::<Two>();
    readers.add_listed::<OtherTwo>();
    readers.add::<Bag>(None);
    let value = |text: &str| {
        let mut raw = Toml::parse::<TomlTable>(&format!("x = {text}")).unwrap();
        raw.take_value("x").unwrap()
    };
    let shown = readers.listed::<Shown>("two").unwrap();
    let other = readers.listed::<Others>("two").unwrap();
    let two = value("{ value = 2 }");
    assert_eq!(
        format!("{:?}", shown(two.clone(), &readers).unwrap()),
        "Two { value: 2 }"
    );
    assert_eq!(
        format!("{:?}", other(two, &readers).unwrap()),
        "OtherTwo { value: 2 }"
    );
    let refused = shown(value("{ value = \"x\" }"), &readers).unwrap_err();
    assert_eq!(
        refused.message(),
        "invalid type: string \"x\", expected i64"
    );
    assert!(readers.listed::<Shown>("three").is_none());
    assert_eq!(readers.listed_keys::<Shown>().collect::<Vec<_>>(), ["two"]);
    assert_eq!(readers.listed_keys::<Others>().collect::<Vec<_>>(), ["two"]);
    // A map's `bag` reads its item with the readers its table's read holds.
    let raw = Toml::parse::<TomlTable>("bag = { item = { value = 7 } }").unwrap();
    let read = raw.read_with(readers.table(SectionTable::Map)).unwrap();
    assert_eq!(
        read.sections.get::<Bag>(),
        Some(&Bag("Two { value: 7 }".to_owned()))
    );
}

#[test]
#[should_panic(expected = "two members of one set take the key two")]
fn two_members_of_one_set_take_no_key_twice() {
    let mut readers = readers();
    readers.add_listed::<Two>();
    readers.add_listed::<Two>();
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
