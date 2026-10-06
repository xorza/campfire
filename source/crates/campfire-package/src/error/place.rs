use std::fmt;

use campfire_capabilities::{DeclaredName, PackagePath};

/// Where in a package a load problem is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    UnitType(DeclaredName),
    /// By its package's name.
    Avatar(String),
    Action(DeclaredName),
    Modifier(DeclaredName),
    Script(PackagePath),
    /// The map's paths.
    Paths,
    /// The mode's `[combat]`.
    Combat,
    /// The mode's `[navigation]`.
    Navigation,
    /// The mode's pool of that name.
    Pool(DeclaredName),
    /// The mode's choice of that name.
    Choice(DeclaredName),
    /// The mode's `[tracks]`.
    Tracks,
    /// The mode's unit types and its avatars, each by its name.
    UnitTypes,
    /// The actions of the mode's loadout packages.
    Loadouts,
    /// The mode's players' resources, beside its pools.
    Resources,
    /// The mode's slot kinds.
    SlotKinds,
    /// The mode's item type of that id.
    Item(DeclaredName),
    /// The mode's `[shop]`.
    Shop,
    /// The mode's `data/mode.toml`.
    Mode,
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Place::UnitType(name) => write!(f, "unit type {name}"),
            Place::Avatar(name) => write!(f, "avatar {name}"),
            Place::Action(id) => write!(f, "action {id}"),
            Place::Modifier(id) => write!(f, "modifier {id}"),
            Place::Script(path) => write!(f, "{path}"),
            Place::Paths => f.write_str("the map's paths"),
            Place::Combat => f.write_str("the mode's [combat]"),
            Place::Navigation => f.write_str("the mode's [navigation]"),
            Place::Pool(name) => write!(f, "pool {name}"),
            Place::Choice(name) => write!(f, "choice {name}"),
            Place::Tracks => f.write_str("the mode's [tracks]"),
            Place::UnitTypes => f.write_str("the mode's unit types and avatars"),
            Place::Loadouts => f.write_str("the mode's loadouts"),
            Place::Resources => f.write_str("the mode's resources and pools"),
            Place::SlotKinds => f.write_str("the mode's slot kinds"),
            Place::Item(id) => write!(f, "item {id}"),
            Place::Shop => f.write_str("the mode's [shop]"),
            Place::Mode => f.write_str("the mode's data/mode.toml"),
        }
    }
}
