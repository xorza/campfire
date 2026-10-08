use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_sim::StableId;

use crate::actions::capability_does::CapabilityDoes;
use crate::progression::Progression;
use crate::progression::progression_column::ProgressionColumn;
use crate::scripts::effects::Effect;
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::frame::Frame;
use crate::units::script_view::View;
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

impl ProgressionEffect {
    /// Queues the listed `does`, experience, to `unit` in `frame`; a unit `view` does not hold,
    /// or that does not have the track, fails the call, as `ctx.add_xp` does.
    pub(crate) fn queue_listed(
        does: CapabilityDoes,
        unit: StableId,
        _: Option<StableId>,
        frame: &mut Frame,
        view: &View,
    ) -> Result<(), CallError> {
        let CapabilityDoes::Xp { track, amount } = does else {
            unreachable!("progression queues only its own listed effects")
        };
        if !ProgressionColumn::has(view, unit, track) {
            return Err(CallError::Api(ApiError::NoTrack));
        }
        frame.effects.push(ProgressionEffect::AddXp {
            unit,
            track,
            amount: amount.number(frame),
        });
        Ok(())
    }
}

impl Effect for ProgressionEffect {
    fn apply(self, world: &mut World, _: &mut Frame, _: Tick) {
        Progression::apply(world, self);
    }
}
