use crate::mode::mode_map::ModeMap;
use crate::navigation::walker::Walker;

/// What `Mode::load` takes from the books beside the units: the mode's map, each kind of unit
/// that walks, by its layer and its body's radius, in order, each once, which the map's pathing
/// grid clears, and the ranks of every loadout entry.
#[derive(Debug)]
pub struct ModeLoad {
    pub(crate) map: ModeMap,
    pub(crate) walkers: Vec<Walker>,
    pub(crate) loadout_ranks: u8,
}
