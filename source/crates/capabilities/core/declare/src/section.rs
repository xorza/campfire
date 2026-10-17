use std::fmt::Debug;

use serde::de::DeserializeOwned;

/// A key of a data table that a module reads as its own type, which it registers by that key, so
/// no table lists the sections of every module (02-engine-core.md, A table's sections are its
/// modules').
pub trait Section: DeserializeOwned + Clone + Eq + Debug + Send + Sync + 'static {
    /// The table it is a key of.
    const TABLE: SectionTable;

    /// Its key in its table.
    const KEY: &'static str;
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
    /// The table whose keys it reads: a delivery type's are a unit type's.
    #[must_use]
    pub(crate) const fn read_in(self) -> Self {
        match self {
            Self::DeliveryType | Self::AnyType => Self::UnitType,
            table => table,
        }
    }
}
