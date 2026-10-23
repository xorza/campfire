use campfire_common::DeclaredName;
use campfire_values::PackagePath;
use derive_more::Display;

use crate::package_name::PackageName;

/// Where in a package a load problem is, each by the path its data names it by.
#[derive(Debug, Display, Clone, PartialEq, Eq)]
pub enum Place {
    #[display("unit type {_0}")]
    UnitType(DeclaredName),
    /// By its package's name.
    #[display("avatar {_0}")]
    Avatar(PackageName),
    /// The entry `name` of a package's table `table`, as `actions`, or of the mode's, as `pools`.
    #[display("[{table}.{name}]")]
    Entry {
        table: &'static str,
        name: DeclaredName,
    },
    /// The mode's section of the key.
    #[display("the mode's [{_0}]")]
    Section(&'static str),
    /// The map's section of the key.
    #[display("the map's [{_0}]")]
    MapSection(&'static str),
    #[display("{_0}")]
    Script(PackagePath),
    /// The mode's choice of that name.
    #[display("choice {_0}")]
    Choice(DeclaredName),
    /// The mode's team of that name.
    #[display("team {_0}")]
    Team(DeclaredName),
    /// The mode's unit types and its avatars, each by its name.
    #[display("the mode's unit types and avatars")]
    UnitTypes,
    /// The actions of the mode's loadout packages.
    #[display("the mode's loadouts")]
    Loadouts,
    /// The mode's map, its units and markers.
    #[display("the map")]
    Map,
    /// The mode's `data/mode.toml`.
    #[display("the mode's data/mode.toml")]
    Mode,
}
