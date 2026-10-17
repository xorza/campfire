use std::any::Any;
use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_sim::TickRate;
use campfire_values::{EngineTag, PackageIndex, PackagePath, TypeScope, UnitType};

use crate::capability_set::CapabilitySet;
use crate::error::TypePlace;
use crate::section::sections::Sections;
use crate::unit_type_data::UnitTypeData;

/// What a match's books are built from, which the package load checked: the match's tick rate and
/// capabilities, the release's engine tags and every other tag the packages name, in the order a
/// match declares them, the mode's table and its map's, each package in the order a match loads
/// them, and what the load gives beside them, by type.
#[derive(Debug)]
pub struct BookSource<'a> {
    pub rate: TickRate,
    pub set: CapabilitySet,
    pub engine_tags: &'a [EngineTag],
    pub mode: &'a Sections,
    pub map: &'a Sections,
    /// The mode's package, then each dependency's.
    pub packages: Vec<SourcePackage<'a>>,
    /// The script the mode runs as its own; none in a match of no mode script, as a test's.
    pub mode_script: Option<ScriptAt<'a>>,
    pub given: Given<'a>,
}

/// A script of the match, by its package's place and its path there.
#[derive(Debug, Clone, Copy)]
pub struct ScriptAt<'a> {
    pub package: PackageIndex,
    pub path: &'a PackagePath,
}

/// One package of a match: its name, its kind, its unit types by name, the sections of its
/// content, and the paths of its scripts, in the order a match compiles them.
#[derive(Debug, Clone)]
pub struct SourcePackage<'a> {
    pub name: &'a str,
    pub kind: SourceKind<'a>,
    pub units: &'a BTreeMap<DeclaredName, UnitTypeData>,
    pub content: &'a Sections,
    pub scripts: Vec<&'a PackagePath>,
}

/// What a package is to its mode: the mode, an avatar and its one unit type, a loadout, or a rules
/// package, whose content is empty, as the load checks.
#[derive(Debug, Clone, Copy)]
pub enum SourceKind<'a> {
    Mode,
    Avatar(&'a UnitTypeData),
    Loadout,
    Rules,
}

/// What the load gives the books beside the packages, each by its type, as a match's compiled
/// scripts.
#[derive(Debug, Default)]
pub struct Given<'a>(Vec<&'a (dyn Any + Send + Sync)>);

impl<'a> Given<'a> {
    /// Gives `value`, the first of its type.
    pub fn add<T: Send + Sync + 'static>(&mut self, value: &'a T) {
        assert!(self.get::<T>().is_none(), "one value of a type is given");
        self.0.push(value);
    }

    /// The value of type `T`, if the load gives one.
    pub fn get<T: 'static>(&self) -> Option<&'a T> {
        self.0.iter().find_map(|value| value.downcast_ref())
    }
}

/// A unit type of the match as its package gives it, the id of each its place in the order a
/// match declares them: each package's types by name, in its scope, then an avatar package's
/// avatar, in the mode's.
#[derive(Debug, Clone, Copy)]
pub struct SourceType<'a> {
    pub unit_type: UnitType,
    pub package: PackageIndex,
    pub scope: TypeScope,
    /// Its name in its scope: the name its package declares it by, or its avatar package's.
    pub name: &'a str,
    pub data: &'a UnitTypeData,
    /// The name its package declares it by, none for the avatar its package stands as.
    pub declared: Option<&'a DeclaredName>,
}

impl SourceType<'_> {
    /// Its place, as a book error names it.
    pub fn place(&self) -> TypePlace {
        self.declared
            .map_or(TypePlace::Avatar, |name| TypePlace::Declared(name.clone()))
    }

    /// Whether it is the avatar its package stands as.
    pub const fn avatar(&self) -> bool {
        self.declared.is_none()
    }

    /// Whether it stands: no delivery type does, which only actions deliver.
    pub const fn stands(&self) -> bool {
        !self.data.delivers()
    }
}
