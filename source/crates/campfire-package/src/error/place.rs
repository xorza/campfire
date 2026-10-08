use crate::files::package_name::PackageName;
use campfire_capabilities::{DeclaredName, PackagePath};
use derive_more::Display;

/// Where in a package a load problem is.
#[derive(Debug, Display, Clone, PartialEq, Eq)]
pub enum Place {
    #[display("unit type {_0}")]
    UnitType(DeclaredName),
    /// By its package's name.
    #[display("avatar {_0}")]
    Avatar(PackageName),
    #[display("action {_0}")]
    Action(DeclaredName),
    #[display("modifier {_0}")]
    Modifier(DeclaredName),
    #[display("{_0}")]
    Script(PackagePath),
    /// The map's paths.
    #[display("the map's paths")]
    Paths,
    /// The mode's `[combat]`.
    #[display("the mode's [combat]")]
    Combat,
    /// The mode's `[navigation]`.
    #[display("the mode's [navigation]")]
    Navigation,
    /// The mode's pool of that name.
    #[display("pool {_0}")]
    Pool(DeclaredName),
    /// The mode's choice of that name.
    #[display("choice {_0}")]
    Choice(DeclaredName),
    /// The mode's `[tracks]`.
    #[display("the mode's [tracks]")]
    Tracks,
    /// The mode's unit types and its avatars, each by its name.
    #[display("the mode's unit types and avatars")]
    UnitTypes,
    /// The actions of the mode's loadout packages.
    #[display("the mode's loadouts")]
    Loadouts,
    /// The mode's players' resources, beside its pools.
    #[display("the mode's resources and pools")]
    Resources,
    /// The mode's slot kinds.
    #[display("the mode's slot kinds")]
    SlotKinds,
    /// The mode's item type of that id.
    #[display("item {_0}")]
    Item(DeclaredName),
    /// The mode's `[shop]`.
    #[display("the mode's [shop]")]
    Shop,
    /// The mode's `[stats]`.
    #[display("the mode's [stats]")]
    Stats,
    /// The mode's `[supply]`.
    #[display("the mode's [supply]")]
    Supply,
    /// The map's `[navigation]`, its pathing grid.
    #[display("the map's [navigation]")]
    MapNavigation,
    /// The map's `[grid]`, its vision grid.
    #[display("the map's [grid]")]
    MapGrid,
    /// The mode's `data/mode.toml`.
    #[display("the mode's data/mode.toml")]
    Mode,
}
