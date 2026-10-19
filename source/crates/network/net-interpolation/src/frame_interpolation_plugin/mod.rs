//! This module is not related to the interpolating between server updates.
//! Instead, it is responsible for interpolating between FixedUpdate ticks during the Update state.
//!
//! Usually, the simulation is run during the FixedUpdate schedule so that it doesn't depend on
//! frame rate. This can cause some visual inconsistencies (jitter) because the frames (Update
//! schedule) don't correspond exactly to the FixedUpdate schedule: there can be frames with several
//! fixed-update ticks, and some frames with no fixed-update ticks.
//!
//! To solve this, we visually display the state of the game with 1 tick of delay.
//! For example if on the Update state we have an overstep of 0.7 and the current tick is 10,
//! we display the state of the game interpolated at 0.7 between tick 9 and tick 10.
//!
//! Another way to solve this would be to run an extra 'partial' simulation step with 0.7 dt and use
//! this for the visual state.
//!
//! To enable FrameInterpolation:
//! - register an interpolation rule for the component or bundle. Existing
//!   interpolation rules are reused by default; if the component should not use
//!   delayed interpolation history, register it with
//!   [`InterpolationFns::no_history`](crate::interpolation_fns::InterpolationFns::no_history).
//! - add [`FrameInterpolationPlugin`]: the prediction plugin adds it, for its correction
//! - To enable FrameInterpolation on a given entity, add the [`FrameInterpolate`](crate::frame_interpolate::FrameInterpolate) marker to it manually
//!
//! Frame interpolation writes through Bevy's change-detection-aware component
//! access. Systems ordered after [`FrameInterpolationSystems::Interpolate`] can
//! therefore use `Changed<C>` to react to the visual value.
//! ```rust,ignore
//! # use campfire_net_interpolation::{
//! #     AppInterpolationExt, FrameInterpolate, FrameInterpolationPlugin, InterpolationFns,
//! #     InterpolationRule,
//! # };
//! # use bevy_app::App;
//! # use bevy_ecs::prelude::*;
//!
//! # #[derive(Component, PartialEq, Clone, Debug)]
//! # struct Component1(f32);
//! # fn lerp_component(start: &Component1, end: &Component1, t: f32) -> Component1 {
//! #     Component1(start.0 + (end.0 - start.0) * t)
//! # }
//!
//! let mut app = App::new();
//! app.add_plugins(FrameInterpolationPlugin);
//! app.register_interpolation_rule::<Component1, ()>(InterpolationRule::new(
//!     InterpolationFns::no_history(lerp_component),
//! ));
//!
//! fn spawn_entity(mut commands: Commands) {
//!     commands.spawn(FrameInterpolate);
//! }
//! ```

use crate::frame_interpolation_systems::FrameInterpolationSystems;
use crate::frame_stage::FrameStage;
use crate::interpolation_owners::InterpolationOwners;
use crate::interpolation_registry::InterpolationRegistry;
use crate::interpolation_registry_plugin::InterpolationRegistryPlugin;
use bevy_app::prelude::*;
use bevy_ecs::archetype::Archetypes;
use bevy_ecs::component::Components;
use bevy_ecs::prelude::*;
use campfire_net_replication::ReplicationSystems;
use campfire_net_timeline::rollback;

/// Bevy plugin that enables frame interpolation.
///
/// This plugin adds systems to store fixed-update component values and then
/// interpolate between the previous and current fixed tick during `PostUpdate`,
/// using [`Time<Fixed>`](bevy_time::Time)'s overstep. This helps smooth visuals when the
/// rendering framerate doesn't align perfectly with the fixed simulation rate.
///
/// To use this, register an interpolation rule in the
/// [`InterpolationRegistry`](crate::interpolation_registry::InterpolationRegistry) and add
/// [`FrameInterpolate`](crate::frame_interpolate::FrameInterpolate) to entities for which you want
/// visual interpolation.
///
/// # Examples
///
/// ```rust,ignore
/// use bevy_app::App;
/// use bevy_ecs::prelude::*;
/// use campfire_net_interpolation::{
///     AppInterpolationExt, FrameInterpolationPlugin, InterpolationFns, InterpolationRule,
/// };
///
/// #[derive(Component, Clone, PartialEq)]
/// struct Position(f32);
///
/// fn lerp_position(start: &Position, end: &Position, t: f32) -> Position {
///     Position(start.0 + (end.0 - start.0) * t)
/// }
///
/// let mut app = App::new();
/// app.add_plugins(FrameInterpolationPlugin);
/// app.register_interpolation_rule::<Position, ()>(InterpolationRule::new(InterpolationFns::no_history(lerp_position)));
/// ```
#[derive(Debug, Default)]
pub struct FrameInterpolationPlugin;

impl Plugin for FrameInterpolationPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<InterpolationRegistryPlugin>() {
            app.add_plugins(InterpolationRegistryPlugin);
        }
        app.configure_sets(
            RunFixedMainLoop,
            FrameInterpolationSystems::Restore.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
        );
        // A replay's ticks are not rendered, so they record no frame history: an entity whose
        // values a rollback replays reads them through its frame source.
        app.configure_sets(
            FixedPostUpdate,
            FrameInterpolationSystems::Update.run_if(not(rollback::is_active)),
        );
        app.configure_sets(
            PostUpdate,
            FrameInterpolationSystems::Interpolate
                .before(bevy_transform::TransformSystems::Propagate)
                // We don't want the visual interpolation value to be the one replicated.
                .after(ReplicationSystems::Send),
        );

        // Each rule and component installs its typed systems in these stages once the registry
        // builds it; the owners resolve the archetypes before each.
        app.init_resource::<InterpolationOwners>();
        app.configure_sets(
            RunFixedMainLoop,
            (FrameStage::Refresh, FrameStage::Run)
                .chain()
                .in_set(FrameInterpolationSystems::Restore),
        );
        app.configure_sets(
            FixedPostUpdate,
            (FrameStage::Refresh, FrameStage::Run)
                .chain()
                .in_set(FrameInterpolationSystems::Update),
        );
        app.configure_sets(
            PostUpdate,
            (FrameStage::Refresh, FrameStage::Run)
                .chain()
                .in_set(FrameInterpolationSystems::Interpolate),
        );
        app.add_systems(
            RunFixedMainLoop,
            refresh_frame_owners.in_set(FrameStage::Refresh),
        );
        app.add_systems(
            FixedPostUpdate,
            refresh_frame_owners.in_set(FrameStage::Refresh),
        );
        app.add_systems(PostUpdate, refresh_frame_owners.in_set(FrameStage::Refresh));
    }
}

/// Resolves the frame interpolation of the archetypes created since the last refresh.
fn refresh_frame_owners(
    mut owners: ResMut<'_, InterpolationOwners>,
    archetypes: &Archetypes,
    components: &Components,
    registry: Res<'_, InterpolationRegistry>,
) {
    registry.refresh_owners(&mut owners.frame, archetypes, components);
}

#[cfg(test)]
mod tests;
