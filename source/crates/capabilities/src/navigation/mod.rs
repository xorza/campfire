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
use crate::navigation::pathing_grid::{PathingGrid, StaticBody};
use crate::navigation::paths::Paths;
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

/// The `navigation` capability: units that walk to a destination, and the map's waypoint paths.
#[derive(Debug)]
pub struct Navigation;

impl Navigation {
    /// Adds navigation to a match, with no paths and the world for bounds until the mode sets its
    /// map's: in Move, units walk towards their destination; in Collide, overlapping living
    /// bodies part; after Collide, each unit that walks stands within the bounds again.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        if let Some(view) = world.get_non_send::<View>() {
            view.add_source(fill_row);
        }
        world.insert_resource(Paths::default());
        world.insert_resource(Bounds::WORLD);
        schedule.add_systems((
            track_static_bodies.in_set(SimSet::Inputs),
            move_units.in_set(SimSet::Move),
            collide.in_set(SimSet::Collide),
            keep_in_bounds.after(SimSet::Collide).before(SimSet::Hit),
        ));
        registry.register_component::<Destination>();
        registry.register_component::<PathWalker>();
        registry.register_component::<MoveStep>();
        registry.register_component::<OnPath>();
    }
}

impl Navigation {
    /// Gives the match the map's pathing grid over `cells`, for walkers of `radii`; the static
    /// bodies mark it from the first tick on.
    pub(crate) fn load_pathing(world: &mut World, cells: Grid, radii: Vec<Num>) {
        world.insert_resource(PathingGrid::new(cells, radii));
    }
}

/// Marks the pathing grid, as each tick starts, with the static bodies: the living units that
/// cannot walk, those the client only holds among them. It builds the grid again only when they
/// changed, so a structure that died or spawned in the tick before counts from this one. Every
/// tick reads them into `statics`, a buffer it keeps.
fn track_static_bodies(
    grid: Option<ResMut<'_, PathingGrid>>,
    bodies: Query<
        '_,
        '_,
        (&StableId, &Position, &Body),
        (Without<MoveStep>, Without<Dead>, Allow<Unpredicted>),
    >,
    mut statics: Local<'_, Vec<StaticBody>>,
) {
    let Some(mut grid) = grid else {
        return;
    };
    statics.clear();
    statics.extend(bodies.iter().map(|(&id, &at, body)| StaticBody {
        id,
        at,
        radius: body.radius(),
    }));
    statics.sort_unstable_by_key(|body| body.id);
    grid.update(&statics);
}

/// Fills a row of the script view with the path the unit walks or stands on.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    fill.row.path = unit.get::<OnPath>().map(|path| path.get());
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
/// pushed, and one walking to a destination yields to one that stands. A predicting client also
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
    let contacts = broadphase.contacts(&colliders);
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
