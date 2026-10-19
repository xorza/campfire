use campfire_declare::{CapabilitySet, ScriptAt, SourcePackage};
use campfire_mode::{MapData, ModeData, TeamManifest};
use campfire_script::ScriptUses;
use campfire_sim::TickRate;
use campfire_units::{HeightGrid, ScriptBook};
use campfire_values::{EngineTag, Speed};

/// What a match's books are built from, which the package load checked: the mode's data, its
/// map and teams, the rules of its manifest, its packages in the order a match loads them, and
/// the hooks of every script, in the order a match compiles them.
#[derive(Debug)]
pub struct BookInput<'a> {
    pub data: &'a ModeData,
    pub map: &'a MapData,
    /// The map's height grid, when it has one.
    pub heights: Option<&'a HeightGrid>,
    /// The playing teams, in the manifest's order.
    pub teams: &'a [TeamManifest],
    pub max_move_speed: Speed,
    /// The capabilities the mode declares, whose modules build the books.
    pub set: CapabilitySet,
    /// The release's engine tags, the first tags of every match.
    pub engine_tags: &'a [EngineTag],
    /// The mode's package, then each dependency's.
    pub packages: Vec<SourcePackage<'a>>,
    pub scripts: &'a ScriptBook,
    /// What each script uses of the data, in the order a match compiles them.
    pub script_uses: &'a ScriptUses,
    /// The scripts the mode's list names, in its order; none in a match of no mode script.
    pub mode_scripts: Vec<ScriptAt<'a>>,
    pub rate: TickRate,
}
