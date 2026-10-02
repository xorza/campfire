use campfire_content::PackagePath;
use campfire_sim::TickRate;

use crate::books::package_content::PackageContent;
use crate::books::unit_type_file::UnitTypeFile;
use crate::mode::map_data::MapData;
use crate::mode::mode_data::ModeData;
use crate::mode::team_manifest::TeamManifest;
use crate::scripts::script_book::ScriptBook;
use crate::stats::stat_id::StatId;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::speed::Speed;

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
    pub tag_names: Vec<&'a str>,
    /// The mode's package, then each dependency's.
    pub packages: Vec<BookPackage<'a>>,
    pub scripts: &'a ScriptBook,
    pub rate: TickRate,
    /// The places of the mode's stats in the order the stats refresh computes them, which the
    /// mode's stat graph gives.
    pub stat_order: Vec<StatId>,
}

impl BookInput<'_> {
    /// The damage kind `name`, which the load checked the mode declares.
    pub(crate) fn damage_kind(&self, name: &DeclaredName) -> DamageKind {
        let kinds = &self.data.combat.damage_kinds;
        let at = kinds.iter().position(|kind| kind == name);
        let at = u8::try_from(at.expect("the load checked the damage kind"));
        DamageKind::new(at.expect("the load keeps damage kinds within u8"))
    }
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

/// What a package is to its mode: the mode, an avatar and its one unit type, or a loadout.
#[derive(Debug, Clone, Copy)]
pub enum BookKind<'a> {
    Mode,
    Avatar(&'a UnitTypeFile),
    Loadout,
}
