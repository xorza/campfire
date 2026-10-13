use crate::mode_load::ModeLoad;
use crate::mode_units::ModeUnits;

/// What the mode's install takes from the books: its unit types, avatars and loadout, and what
/// `Mode::load` takes beside them.
#[derive(Debug)]
pub struct ModeInputs {
    pub units: ModeUnits,
    pub load: ModeLoad,
}
