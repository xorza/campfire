use crate::mode_map::ModeMap;

/// What `Mode::load` takes from the books beside the units: the mode's map, and the ranks of
/// every loadout entry.
#[derive(Debug)]
pub struct ModeLoad {
    pub map: ModeMap,
    pub loadout_ranks: u8,
}
