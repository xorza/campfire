//! Frame interpolation: the fixed ticks run at their own rate, so a frame shows each component one
//! tick late, between its last two ticks at the fixed overstep, rather than step a partial tick.
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ScheduleLabel;

use crate::frame_holders::FrameHolders;
use crate::frame_interpolation_systems::FrameInterpolationSystems;
use crate::frame_phase_systems::FramePhaseSystems;

/// The frame pipeline of a client: each frame rule writes its visual in `PostUpdate`, before the
/// transforms propagate, through change detection, so a later system reads `Changed<V>`; the frame
/// histories record after each fixed tick. The rules install their systems as
/// [`FrameRegistryPlugin`](crate::FrameRegistryPlugin) builds them.
#[derive(Debug, Default)]
pub struct FrameInterpolationPlugin;

impl Plugin for FrameInterpolationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameHolders>();
        app.configure_sets(
            PostUpdate,
            FrameInterpolationSystems::Interpolate
                .before(bevy_transform::TransformSystems::Propagate),
        );
        for (schedule, set) in [
            (FixedPostUpdate.intern(), FrameInterpolationSystems::Update),
            (PostUpdate.intern(), FrameInterpolationSystems::Interpolate),
        ] {
            app.configure_sets(
                schedule,
                (FramePhaseSystems::Refresh, FramePhaseSystems::Run)
                    .chain()
                    .in_set(set),
            );
            app.add_systems(
                schedule,
                FrameHolders::refresh.in_set(FramePhaseSystems::Refresh),
            );
        }
        app.configure_sets(
            PostUpdate,
            FramePhaseSystems::Settle
                .after(FramePhaseSystems::Run)
                .in_set(FrameInterpolationSystems::Interpolate),
        );
        app.add_systems(
            PostUpdate,
            FrameHolders::refresh.in_set(FramePhaseSystems::Settle),
        );
    }
}

#[cfg(test)]
mod tests;
