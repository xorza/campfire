use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Allow, Has, With, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::{EntityRef, World};
use campfire_math::Num;
use campfire_sim::{Position, SimSet, StableId, StateRegistry, Unpredicted};

use crate::combat::dead::Dead;
use crate::navigation::broadphase::Broadphase;
use crate::navigation::collider::Collider;
use crate::navigation::destination::Destination;
use crate::navigation::move_step::MoveStep;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::paths::Paths;
use crate::navigation::route::{Route, Waiting};
use crate::navigation::route_planner::RoutePlanner;
use crate::navigation::static_index::{StaticBody, StaticIndex};
use crate::units::body::Body;
use crate::units::script_view::{RowFill, View};
use crate::values::bounds::Bounds;
use crate::values::grid::Grid;

#[cfg(feature = "bench")]
pub(crate) mod bench;
pub(crate) mod broadphase;
pub(crate) mod collider;
pub(crate) mod destination;
pub(crate) mod move_step;
pub(crate) mod on_path;
pub(crate) mod path_walker;
pub(crate) mod pathing_grid;
pub(crate) mod paths;
pub(crate) mod route;
pub(crate) mod route_planner;
pub(crate) mod static_index;

/// The `navigation` capability: units that walk to a destination, and the map's waypoint paths.
#[derive(Debug)]
pub struct Navigation;

impl Navigation {
    /// Adds navigation to a match, with no paths, the world for bounds, and a static index for
    /// walkers as wide as a body may be, until the mode sets its map's: in Move, units walk
    /// towards their destination; in Collide, overlapping living bodies part; after Collide, each
    /// unit that walks stands within the bounds again.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        if let Some(view) = world.get_non_send::<View>() {
            view.add_source(fill_row);
        }
        world.insert_resource(Paths::default());
        world.insert_resource(Bounds::WORLD);
        world.insert_resource(StaticIndex::new(Body::MAX_RADIUS));
        schedule.add_systems((
            track_static_bodies.in_set(SimSet::Inputs),
            (plan_routes, move_units).chain().in_set(SimSet::Move),
            (track_static_bodies, collide)
                .chain()
                .in_set(SimSet::Collide),
            keep_in_bounds.after(SimSet::Collide).before(SimSet::Hit),
        ));
        registry.register_component::<Destination>();
        registry.register_component::<PathWalker>();
        registry.register_component::<MoveStep>();
        registry.register_component::<OnPath>();
        registry.register_component::<Route>();
    }
}

impl Navigation {
    /// Gives the match the map's pathing grid over `cells`, for walkers of `radii`, 0 for one
    /// with no body, a static index for the widest of them, and a planner of routes on the grid;
    /// the static bodies fill the grid and the index from the first tick on.
    pub(crate) fn load_pathing(world: &mut World, cells: Grid, radii: Vec<Num>) {
        let widest = radii
            .iter()
            .max()
            .copied()
            .filter(|&widest| widest > Num::ZERO);
        world.insert_resource(StaticIndex::new(widest.unwrap_or(Body::MAX_RADIUS)));
        world.insert_resource(RoutePlanner::new(&cells));
        world.insert_resource(PathingGrid::new(cells, radii));
    }
}

/// Gives the static index and the pathing grid the static bodies: the living units that cannot
/// walk, those the client only holds among them. It runs as each tick starts, so a structure that
/// died or spawned in the tick before counts from this one, and again as Collide starts, so
/// collision parts walkers from the static bodies as they stand then. Every run reads them into
/// `statics`, a buffer it keeps.
fn track_static_bodies(
    mut index: ResMut<'_, StaticIndex>,
    grid: Option<ResMut<'_, PathingGrid>>,
    bodies: Query<
        '_,
        '_,
        (&StableId, &Position, &Body),
        (Without<MoveStep>, Without<Dead>, Allow<Unpredicted>),
    >,
    mut statics: Local<'_, Vec<StaticBody>>,
) {
    statics.clear();
    statics.extend(bodies.iter().map(|(&id, &at, body)| StaticBody {
        id,
        at,
        radius: body.radius(),
    }));
    statics.sort_unstable_by_key(|body| body.id);
    if index.update(&statics)
        && let Some(mut grid) = grid
    {
        grid.update(&index);
    }
}

/// Fills a row of the script view with the path the unit walks or stands on.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    fill.row.path = unit.get::<OnPath>().map(|path| path.get());
}

/// Plans the asked routes, by the tick they were asked in, then by stable id, until the tick has
/// expanded as many cells as the pathing grid has: the route that meets that limit finishes, and
/// the rest wait for the next tick, so a tick plans at most one search over the whole grid past
/// the limit. A match with no pathing grid plans none. Every tick reads the asks into `waiting`,
/// a buffer it keeps.
fn plan_routes(
    grid: Option<Res<'_, PathingGrid>>,
    planner: Option<ResMut<'_, RoutePlanner>>,
    mut units: Query<
        '_,
        '_,
        (Entity, &StableId, &Position, &mut Route, Option<&Body>),
        Without<Dead>,
    >,
    mut waiting: Local<'_, Vec<Waiting>>,
) {
    let (Some(grid), Some(mut planner)) = (grid, planner) else {
        return;
    };
    waiting.clear();
    waiting.extend(units.iter().filter_map(|(entity, &id, _, route, _)| {
        let ask = route.asked()?;
        Some(Waiting {
            tick: ask.tick,
            id,
            entity,
        })
    }));
    waiting.sort_unstable();
    let mut expanded = 0;
    for next in &*waiting {
        if expanded >= grid.cells() {
            break;
        }
        let (_, _, &at, mut route, body) =
            units.get_mut(next.entity).expect("a unit read this tick");
        let goal = route
            .asked()
            .expect("a unit waiting for its route asked one")
            .goal;
        let layer = grid.layer(Body::radius_of(body));
        expanded += planner.plan(layer, at, goal, route.answer()).expanded as usize;
    }
}

/// Walks each unit one step towards its destination, which it drops on arrival. A dead unit stays
/// where it fell, and forgets where it walked to.
fn move_units(mut units: Query<'_, '_, (&mut Position, &mut Destination, &MoveStep, Has<Dead>)>) {
    for (mut position, mut destination, step, dead) in &mut units {
        let Some(target) = destination.get() else {
            continue;
        };
        if dead {
            destination.set(None);
            continue;
        }
        let moved = position.get().step_toward(target.get(), step.get());
        *position = Position::new(moved).expect("a step ends between two points within the bound");
        if moved == target.get() {
            destination.set(None);
        }
    }
}

/// Parts the living bodies that overlap as the stage starts, pair by pair in stable-id order; a
/// pair that only overlaps after this tick's pushes parts in the next. Only a unit that can walk is
/// pushed, and one walking to a destination yields to one that stands. The static bodies' contacts
/// come from `statics`, which holds them as the stage starts. A predicting client also
/// parts its own units from the units it holds as the server sent them that cannot walk, such as
/// towers, which never move. Every other held unit is where the server last had it, behind the
/// client's ticks, and may have started or stopped walking since, so the server alone parts the
/// client's units from it. Every tick reads the bodies into `colliders`, and finds their
/// contacts with `broadphase`, buffers it keeps.
fn collide(
    mut units: Query<
        '_,
        '_,
        (
            Entity,
            &StableId,
            &Body,
            &mut Position,
            Has<MoveStep>,
            Option<&Destination>,
            Has<Unpredicted>,
        ),
        (Without<Dead>, Allow<Unpredicted>),
    >,
    statics: Res<'_, StaticIndex>,
    mut colliders: Local<'_, Vec<Collider>>,
    mut broadphase: Local<'_, Broadphase>,
) {
    colliders.clear();
    colliders.extend(
        units
            .iter()
            .filter(|&(.., movable, _, held)| !(held && movable))
            .map(
                |(entity, &id, body, position, movable, destination, _)| Collider {
                    id,
                    entity,
                    at: position.get(),
                    radius: body.radius(),
                    movable,
                    walking: destination.is_some_and(|destination| destination.get().is_some()),
                },
            ),
    );
    colliders.sort_unstable_by_key(|collider| collider.id);
    debug_assert_eq!(
        colliders
            .iter()
            .filter(|collider| !collider.movable)
            .count(),
        statics.len(),
        "the static index holds the static bodies as the stage starts"
    );
    let contacts = broadphase.contacts(&colliders, &statics);
    Collider::resolve(&mut colliders, contacts);
    for collider in &*colliders {
        let (_, _, _, mut position, ..) = units
            .get_mut(collider.entity)
            .expect("a body read this tick");
        let parted = Position::new(collider.at).expect("a push stays near the bounds");
        position.set_if_neq(parted);
    }
}

/// Clamps each unit that walks into the bounds; a unit already within them does not change.
fn keep_in_bounds(
    bounds: Res<'_, Bounds>,
    mut units: Query<'_, '_, &mut Position, With<MoveStep>>,
) {
    for mut position in &mut units {
        position.set_if_neq(bounds.clamp(*position));
    }
}

#[cfg(test)]
mod tests;
