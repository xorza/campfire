use std::fmt::Debug;
use std::sync::Arc;

use campfire_codec::{StructFields, TomlError, TomlValue};
use campfire_script::{ApiBuilder, DataTable, Status};
use serde::de::DeserializeOwned;

use crate::section::section_readers::SectionReaders;

pub(crate) mod patched;
pub(crate) mod section_readers;
pub(crate) mod section_writer;
pub(crate) mod sections;

/// A key of a data table that a module reads as its own type, which it registers by that key, so
/// no table lists the sections of every module (02-engine-core.md, A table's sections are its
/// modules').
pub trait Section: ReadValue + Clone + Eq + Debug + Send + Sync + 'static {
    /// The table it is a key of.
    const TABLE: SectionTable;

    /// Its key in its table.
    const KEY: &'static str;

    /// The release its key runs since, or that refuses it, as a script name's status says
    /// (06-script-api.md, A name runs since a version).
    const STATUS: Status;
}

/// A data table whose keys modules register as their sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionTable {
    /// The mode's `data/mode.toml`.
    Mode,
    /// A map's `map/<name>/map.toml`.
    Map,
    /// A package's content: its actions, modifiers and item types, each by id, beside its unit
    /// types.
    Package,
    /// A unit type, `[units.<id>]`.
    UnitType,
    /// A unit type's key that makes it a delivery type, whose units only actions deliver and no
    /// map or train places.
    DeliveryType,
    /// A unit type's key that a delivery type holds too, as its tags.
    AnyType,
}

impl SectionTable {
    /// The table of the script API's reference that lists its keys: the mode's file holds its
    /// package's content, and a delivery type's keys are a unit type's; none for a map's, which
    /// the reference does not list.
    #[must_use]
    pub const fn data_table(self) -> Option<DataTable> {
        match self {
            Self::Mode | Self::Package => Some(DataTable::MODE),
            Self::UnitType | Self::DeliveryType | Self::AnyType => Some(DataTable::UNIT_TYPE),
            Self::Map => None,
        }
    }

    /// The table whose keys it reads: a delivery type's are a unit type's.
    #[must_use]
    pub(crate) const fn read_in(self) -> Self {
        match self {
            Self::DeliveryType | Self::AnyType => Self::UnitType,
            table => table,
        }
    }
}

/// How a type reads from a value of a data file: as serde reads it, or, for one that holds the
/// members of an open set, with the release's readers of that set, as an action holds its listed
/// effects.
pub trait ReadValue: Sized {
    fn read_value(value: TomlValue, readers: &SectionReaders) -> Result<Self, TomlError>;

    /// The keys of its table that a mode's default may give, by name: the fields serde reads of a
    /// struct; none for a value that reads no struct by its fields (03-game-scripting.md,
    /// Defaults under each unit type).
    fn fields() -> &'static [&'static str];
}

impl<T: DeserializeOwned> ReadValue for T {
    fn read_value(value: TomlValue, _: &SectionReaders) -> Result<Self, TomlError> {
        value.read()
    }

    fn fields() -> &'static [&'static str] {
        StructFields::of::<T>()
    }
}

/// An open set of values that a data table names by key, each the type of the module that
/// registers it, as the effects of an action's lists: the crate that defines the set names the
/// trait object its members read as, and no type lists the members (02-engine-core.md, A table's
/// sections are its modules').
pub trait ListedSet: 'static {
    /// What each member reads as.
    type Value: ?Sized + Send + Sync + 'static;

    /// The table of the script API reference that lists the members' keys.
    const TABLE: DataTable;
}

/// A member of the set `Set`, which a table names by `KEY`.
pub trait Listed: ReadValue + 'static {
    type Set: ListedSet;

    const KEY: &'static str;

    /// The release its key runs since, or that refuses it, as `Section::STATUS` says.
    const STATUS: Status;

    /// The member as its set holds it.
    fn into_listed(self) -> Arc<<Self::Set as ListedSet>::Value>;

    /// Binds what a script gives of the member beside its key: an effect's call of `ctx`, which
    /// queues the effect value its data queues; nothing for a member no call gives, as an action's
    /// kind (04-capabilities/actions.md, An effect declares its call with its key).
    fn api(api: &mut ApiBuilder<'_>);
}
