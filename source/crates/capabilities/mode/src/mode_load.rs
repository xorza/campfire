use campfire_navigation::Walker;

use crate::mode_map::ModeMap;

/// What `Mode::load` takes from the books beside the units: the mode's map, each kind of unit
/// that walks, by its layer and its body's radius, in order, each once, which the map's pathing
/// grid clears, and the ranks of every loadout entry.
#[derive(Debug)]
pub struct ModeLoad {
    pub map: ModeMap,
    pub walkers: Vec<Walker>,
    pub loadout_ranks: u8,
}
