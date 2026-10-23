use campfire_actions::LoadoutRanks;
use campfire_declarations::{BookCx, BookError};

use crate::mode_load::ModeLoad;
use crate::mode_map::ModeMap;
use crate::mode_units::ModeUnits;

/// What the mode's install takes from the books: its unit types, avatars and loadout, and what
/// `Mode::load` takes beside them.
#[derive(Debug)]
pub struct ModeInputs {
    pub units: ModeUnits,
    pub load: ModeLoad,
}

impl ModeInputs {
    /// Takes the facts the mode's core declares out of the build `cx`, as the match's load reads
    /// them beside the books.
    pub fn take(cx: &mut BookCx<'_>) -> Result<Self, BookError> {
        let units = cx.take_fact::<ModeUnits>()?.unwrap_or_default();
        let map = cx
            .take_fact::<ModeMap>()?
            .expect("the mode's core resolves its map");
        let loadout_ranks = cx.take_fact::<LoadoutRanks>()?.map_or(1, |ranks| ranks.0);
        Ok(Self {
            units,
            load: ModeLoad { map, loadout_ranks },
        })
    }
}
