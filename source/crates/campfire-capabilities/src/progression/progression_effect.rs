use campfire_math::Num;
use campfire_sim::{Capability, StableId};

use crate::scripts::effects::Effect;
use crate::units::track_id::TrackId;

/// A change to units' progress that a call queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgressionEffect {
    /// Experience, not negative, on a track the unit has.
    AddXp {
        unit: StableId,
        track: TrackId,
        amount: Num,
    },
}

impl Effect for ProgressionEffect {
    const CAPABILITY: Capability = Capability::Progression;
}
