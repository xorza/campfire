use std::num::NonZeroU64;

use bevy_ecs::resource::Resource;

use crate::unit_stats::UnitStats;

/// Creep waves on a fixed timer: in tick `first` and every `interval` ticks after it, each side
/// gets `creeps` at the start of every lane. It stands in for the mode's wave timer until mode
/// scripts run. Mode data, not state.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct Waves {
    pub first: u64,
    pub interval: NonZeroU64,
    pub creeps: Vec<UnitStats>,
}

impl Waves {
    pub const fn due(&self, tick: u64) -> bool {
        match tick.checked_sub(self.first) {
            Some(since) => since % self.interval.get() == 0,
            None => false,
        }
    }
}
