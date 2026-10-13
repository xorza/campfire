use campfire_core::ScriptBook;
use campfire_mode::{MapData, ModeData, TeamManifest};
use campfire_sim::TickRate;
use campfire_stats::StatId;
use campfire_values::{DeclaredName, PackagePath, Speed};

use crate::books::package_content::PackageContent;
use crate::books::unit_type_data::UnitTypeData;

/// What a match's books are built from, which the package load checked: the mode's data, its
/// map and teams, the rules of its manifest, its packages in the order a match loads them, and
/// the hooks of every script, in the order a match compiles them.
#[derive(Debug)]
pub struct BookInput<'a> {
    pub data: &'a ModeData,
    pub map: &'a MapData,
    /// The playing teams, in the manifest's order.
    pub teams: &'a [TeamManifest],
    pub max_move_speed: Speed,
    /// Whether the mode declares `progression`, whose tracks then load.
    pub progression: bool,
    /// Every tag the packages name but the engine's, in the order a match declares them.
    pub tag_names: &'a [DeclaredName],
    /// The mode's package, then each dependency's.
    pub packages: Vec<BookPackage<'a>>,
    pub scripts: &'a ScriptBook,
    pub rate: TickRate,
    /// The places of the mode's stats in the order the stats refresh computes them, which the
    /// mode's stat graph gives.
    pub stat_order: &'a [StatId],
}

/// One package of a match: its name, its content, its kind, and the paths of its scripts, in
/// the order a match compiles them.
#[derive(Debug)]
pub struct BookPackage<'a> {
    pub name: &'a str,
    pub content: &'a PackageContent,
    pub kind: BookKind<'a>,
    pub scripts: Vec<&'a PackagePath>,
}

/// What a package is to its mode: the mode, an avatar and its one unit type, a loadout, or a rules
/// package.
#[derive(Debug, Clone, Copy)]
pub enum BookKind<'a> {
    Mode,
    Avatar(&'a UnitTypeData),
    Loadout,
    /// A rules package, whose scripts a mode runs; its content is empty, as the load checks.
    Rules,
}
