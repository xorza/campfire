use campfire_navigation::WalkerKinds;

use crate::mode_map::ModeMap;

/// What `Mode::load` takes from the books beside the units: the mode's map, each kind of unit
/// that walks, which the map's pathing grid clears, and the ranks of every loadout entry.
#[derive(Debug)]
pub struct ModeLoad {
    pub map: ModeMap,
    pub walkers: WalkerKinds,
    pub loadout_ranks: u8,
}
