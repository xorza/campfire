use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_sim::StableId;

use crate::progression::Progression;
use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;
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
    fn apply(self, world: &mut World, _: &mut Frame, _: Tick) {
        Progression::apply(world, self);
    }
}
