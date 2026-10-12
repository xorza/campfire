use bevy_ecs::bundle::Bundle;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_sim::{Capability, SimEdge, SimSet};

use crate::actions::effect_queues::EffectQueues;
use crate::declare::declarations::Declarations;
use crate::declare::declares::Declares;
use crate::declare::install_cx::InstallCx;
use crate::geometry::shape::Shape;
use crate::navigation::body_index::BodyIndex;
use crate::navigation::destination::Destination;
use crate::navigation::forced_moves::ForcedMoves;
use crate::navigation::navigation_api::NavigationApi;
use crate::navigation::navigation_column::{NavigationColumn, RowParts};
use crate::navigation::navigation_effect::NavigationEffect;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::navigation::paths::Paths;
use crate::navigation::progress::Progress;
use crate::navigation::route::Route;
use crate::navigation::routing::Routing;
use crate::navigation::static_changes::StaticChanges;
use crate::navigation::static_tracking::StaticTracking;
use crate::navigation::statics_dirty::StaticsDirty;
use crate::navigation::walker::Walker;
use crate::navigation::walking::Walking;
use crate::units::by_type::ByType;
use crate::units::move_step::MoveStep;

#[cfg(feature = "bench")]
pub(crate) mod bench;
pub(crate) mod body_index;
pub(crate) mod broadphase;
pub(crate) mod collider;
pub(crate) mod destination;
pub(crate) mod error;
pub(crate) mod forced_moves;
pub(crate) mod group_box;
pub(crate) mod navigation_api;
pub(crate) mod navigation_column;
pub(crate) mod navigation_effect;
pub(crate) mod navigation_rules;
pub(crate) mod navigation_world;
pub(crate) mod on_path;
pub(crate) mod party;
pub(crate) mod path_walker;
pub(crate) mod pathing_grid;
pub(crate) mod paths;
pub(crate) mod progress;
pub(crate) mod regions;
pub(crate) mod route;
pub(crate) mod route_asks;
pub(crate) mod route_planner;
pub(crate) mod routing;
pub(crate) mod segment;
pub(crate) mod static_changes;
pub(crate) mod static_tracking;
pub(crate) mod statics_dirty;
pub(crate) mod steering;
pub(crate) mod terrain;
pub(crate) mod walker;
pub(crate) mod walking;
pub(crate) mod wall;
pub(crate) mod walls;

/// The systems of `navigation`, for the systems of other capabilities to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum NavigationSet {
    /// In `SimSet::Inputs`: the static index and the pathing grid take the static bodies as the
    /// tick starts.
    TrackStatics,
}

/// The `navigation` capability: units that walk routes to a destination, the map's waypoint
/// paths, and the forced moves of dashes, knock backs and teleports.
#[derive(Debug)]
pub struct Navigation;

impl Declares for Navigation {
    const MODULE: Option<Capability> = Some(Capability::Navigation);

    fn declare<D: Declarations>(d: &mut D) {
        d.component::<Destination>();
        d.component::<PathWalker>();
        d.component::<OnPath>();
        d.component::<Route>();
        d.component::<Progress>();
        d.book::<ByType<Walker>>();
        d.column::<NavigationColumn, RowParts>(NavigationColumn::fill_row);
        d.api(NavigationApi::register);
    }
}

impl Navigation {
    /// The components of a new unit that walks `step` a tick, with nowhere to go yet.
    pub fn walker(step: MoveStep) -> impl Bundle {
        (
            step,
            Destination::default(),
            Route::default(),
            Progress::default(),
        )
    }

    /// Adds navigation to a match, with no paths, the world for bounds, and a static index for
    /// walkers as wide as a body may be, until the mode sets its map's: in Move, units walk their
    /// routes to their destinations, straight lines until the map gives a pathing grid, then the
    /// forced moves move theirs; in Collide, overlapping living bodies part; after Collide, each
    /// unit that walks stands within the bounds again.
    pub(crate) fn install(cx: &mut InstallCx<'_>) {
        let (world, schedule) = (&mut *cx.world, &mut *cx.schedule);
        world.insert_resource(Paths::default());
        world.insert_resource(BodyIndex::new(Shape::MAX_BOUND));
        world.insert_resource(StaticChanges::default());
        StaticsDirty::install(world);
        world
            .resource_mut::<EffectQueues>()
            .register(Capability::Navigation, NavigationEffect::queue_listed);
        schedule.add_systems((
            StaticTracking::track_static_bodies
                .in_set(SimSet::Inputs)
                .in_set(NavigationSet::TrackStatics),
            (
                Walking::forget_dead,
                Routing::route_units,
                Routing::plan_routes,
                Walking::steer,
                Walking::move_units,
                ForcedMoves::force_units,
            )
                .chain()
                .in_set(SimSet::Move),
            (StaticTracking::track_static_bodies, Walking::collide)
                .chain()
                .in_set(SimSet::Collide),
            Walking::keep_in_bounds.in_set(SimEdge::After(SimSet::Collide)),
        ));
    }
}

#[cfg(test)]
mod tests;
