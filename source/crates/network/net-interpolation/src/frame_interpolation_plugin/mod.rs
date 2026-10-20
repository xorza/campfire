//! Frame interpolation: the fixed ticks run at their own rate, so a frame shows each component one
//! tick late, between its last two ticks at the fixed overstep, rather than step a partial tick.

use crate::frame_interpolation_systems::FrameInterpolationSystems;
use crate::frame_phase_systems::FramePhaseSystems;
use crate::interpolation_holders::InterpolationHolders;
use crate::interpolation_registry::InterpolationRegistry;
use crate::pipelines::Pipelines;
use bevy_app::prelude::*;
use bevy_ecs::archetype::Archetypes;
use bevy_ecs::component::Components;
use bevy_ecs::prelude::*;
use campfire_net_replication::ReplicationSystems;
use campfire_net_timeline::Rollback;

/// Frame interpolation of each entity with
/// [`FrameInterpolate`](crate::frame_interpolate::FrameInterpolate), written in `PostUpdate`
/// through change detection, so a later system reads `Changed<C>`.
#[derive(Debug, Default)]
pub struct FrameInterpolationPlugin;

impl Plugin for FrameInterpolationPlugin {
    fn build(&self, app: &mut App) {
        Pipelines::mark(app, |pipelines| pipelines.frame = true);
        app.configure_sets(
            RunFixedMainLoop,
            FrameInterpolationSystems::Restore.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
        );
        // A replay's ticks are not rendered, so they record no frame history: an entity whose
        // values a rollback replays reads them through its frame source.
        app.configure_sets(
            FixedPostUpdate,
            FrameInterpolationSystems::Update.run_if(not(Rollback::is_active)),
        );
        app.configure_sets(
            PostUpdate,
            FrameInterpolationSystems::Interpolate
                .before(bevy_transform::TransformSystems::Propagate)
                // We don't want the visual interpolation value to be the one replicated.
                .after(ReplicationSystems::Send),
        );

        // Each rule and component installs its typed systems in these stages once the registry
        // builds it; the holders resolve the archetypes before each.
        app.init_resource::<InterpolationHolders>();
        app.configure_sets(
            RunFixedMainLoop,
            (FramePhaseSystems::Refresh, FramePhaseSystems::Run)
                .chain()
                .in_set(FrameInterpolationSystems::Restore),
        );
        app.configure_sets(
            FixedPostUpdate,
            (FramePhaseSystems::Refresh, FramePhaseSystems::Run)
                .chain()
                .in_set(FrameInterpolationSystems::Update),
        );
        app.configure_sets(
            PostUpdate,
            (FramePhaseSystems::Refresh, FramePhaseSystems::Run)
                .chain()
                .in_set(FrameInterpolationSystems::Interpolate),
        );
        app.add_systems(
            RunFixedMainLoop,
            Self::refresh_frame_holders.in_set(FramePhaseSystems::Refresh),
        );
        app.add_systems(
            FixedPostUpdate,
            Self::refresh_frame_holders.in_set(FramePhaseSystems::Refresh),
        );
        app.add_systems(
            PostUpdate,
            Self::refresh_frame_holders.in_set(FramePhaseSystems::Refresh),
        );
    }
}

impl FrameInterpolationPlugin {
    /// Resolves the frame interpolation of the archetypes created since the last refresh.
    fn refresh_frame_holders(
        mut holders: ResMut<'_, InterpolationHolders>,
        archetypes: &Archetypes,
        components: &Components,
        registry: Res<'_, InterpolationRegistry>,
    ) {
        registry.refresh_holders(&mut holders.frame, archetypes, components);
    }
}

#[cfg(test)]
mod tests;
