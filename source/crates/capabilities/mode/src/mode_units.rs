use campfire_units::Spawner;
use campfire_values::{Names, UnitType};

use crate::loadout_setup::LoadoutSetup;
use crate::offer::Offer;

/// The mode's unit types that stand, its avatars' among them, its avatars by their packages'
/// names, in the order the mode depends on them, and its loadout's entries.
#[derive(Debug, Clone, Default)]
pub struct ModeUnits {
    pub unit_types: Vec<UnitType>,
    pub avatars: Names<Offer>,
    pub loadout: LoadoutSetup,
}

impl ModeUnits {
    /// The spawner of the mode's unit types.
    pub(crate) fn spawner(&self) -> Spawner {
        Spawner::new(self.unit_types.iter().copied())
    }
}
