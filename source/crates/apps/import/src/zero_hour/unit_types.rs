use std::collections::BTreeMap;
use std::num::NonZeroU8;

use campfire_codec::Toml;
use campfire_common::DeclaredName;
use campfire_math::Num;
use serde::Serialize;

use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::imported_type::ImportedType;

/// The closed set of the import's unit types, by their names: each the fields its object gives.
/// The stages after it read it and add no type; what they give a type, its actions and its
/// production, they return.
#[derive(Debug)]
pub(crate) struct UnitTypes {
    types: BTreeMap<ImportName, ImportedType>,
}

/// What the stages give each unit type, by its name: the actions of its slot kinds `weapon`, in its
/// starting weapon set's order, and `command`, in its command set's order, and its `production`
/// section when it trains (09-zero-hour.md, Every command set's trains and builds are imported;
/// 09-zero-hour.md, A weapon converts when it hits one target at once).
#[derive(Debug, Clone, Copy)]
pub(crate) struct TypeAdditions<'a> {
    pub(crate) weapon: &'a BTreeMap<ImportName, Vec<DeclaredName>>,
    pub(crate) command: &'a BTreeMap<ImportName, Vec<DeclaredName>>,
    pub(crate) production: &'a BTreeMap<ImportName, ProductionSection>,
}

/// A unit type's `production` section as the import writes it: its queue's length, and its exit
/// and default rally points, `[x, z]` in its own frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct ProductionSection {
    pub(crate) queue: NonZeroU8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) exit: Option<[Num; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rally: Option<[Num; 2]>,
}

/// A unit type's section of `data/units.toml`: the fields its object gives, its slots and its
/// production.
#[derive(Debug, Serialize)]
struct TypeToml<'a> {
    #[serde(flatten)]
    imported: &'a ImportedType,
    #[serde(skip_serializing_if = "Option::is_none")]
    slots: Option<TypeSlots<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    production: Option<ProductionSection>,
}

/// A unit type's `slots`: the actions of its slot kinds `weapon` and `command`.
#[derive(Debug, Serialize)]
struct TypeSlots<'a> {
    #[serde(skip_serializing_if = "<[DeclaredName]>::is_empty")]
    weapon: &'a [DeclaredName],
    #[serde(skip_serializing_if = "<[DeclaredName]>::is_empty")]
    command: &'a [DeclaredName],
}

/// `data/units.toml`: `[units.<name>]` of each type, the fields its object gives, and its slots
/// and production.
#[derive(Debug, Serialize)]
struct UnitsToml<'a> {
    units: BTreeMap<&'a str, TypeToml<'a>>,
}

/// The package's `data/units.toml`, and the largest speed of its types, when one drives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnitsFile {
    pub(crate) text: String,
    pub(crate) max_speed: Option<Num>,
}

impl UnitTypes {
    pub(crate) const fn new(types: BTreeMap<ImportName, ImportedType>) -> Self {
        Self { types }
    }

    /// The package's `data/units.toml`: each unit type in the order of their names, of the
    /// fields its object gives, the slots and the production `additions` give it.
    pub(crate) fn units_file(&self, additions: TypeAdditions<'_>) -> UnitsFile {
        const NONE: &[DeclaredName] = &[];
        let units: BTreeMap<&str, TypeToml<'_>> = self
            .types
            .iter()
            .map(|(name, imported)| {
                let weapon = additions.weapon.get(name).map_or(NONE, Vec::as_slice);
                let command = additions.command.get(name).map_or(NONE, Vec::as_slice);
                let section = TypeToml {
                    imported,
                    slots: (!weapon.is_empty() || !command.is_empty())
                        .then_some(TypeSlots { weapon, command }),
                    production: additions.production.get(name).copied(),
                };
                (name.as_str(), section)
            })
            .collect();
        let max_speed = units
            .values()
            .filter_map(|unit| unit.imported.move_speed())
            .max();
        UnitsFile {
            text: Toml::write(&UnitsToml { units }),
            max_speed,
        }
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use crate::zero_hour::unit_types::{TypeAdditions, UnitTypes, UnitsFile};

    impl UnitTypes {
        /// Its `data/units.toml` of no stage's additions.
        pub(crate) fn bare_file(&self) -> UnitsFile {
            self.units_file(TypeAdditions {
                weapon: &BTreeMap::new(),
                command: &BTreeMap::new(),
                production: &BTreeMap::new(),
            })
        }
    }
}
