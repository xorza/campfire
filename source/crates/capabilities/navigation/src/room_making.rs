use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, With, Without};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{In, Local, SystemId};
use bevy_ecs::world::World;
use campfire_declarations::OneShot;
use campfire_geometry::{Placed, Shape};
use campfire_sim::{Keyed, Position, SimTick, StableId};
use campfire_units::{Body, Dead, MoveStep};

use crate::body_index::BodyIndex;
use crate::navigation_effect::NavigationEffect;
use crate::pathing_grid::PathingGrid;
use crate::route_planner::{RoutePlanner, Walkable};
use crate::static_tracking::StaticTrackingRun;
use crate::walker::Walker;

/// The room a static body makes for itself as it spawns, as `Navigation::make_room` says.
#[derive(Debug)]
pub struct RoomMaking;

/// The room making as a system of its own, which a spawn runs out of the schedule, so its query
/// of the walkers and its list of those a body overlaps persist between its runs.
#[derive(Resource, Debug, Clone, Copy)]
pub struct RoomMakingRun(pub(super) SystemId<In<Entity>>);

impl OneShot for RoomMakingRun {
    type In = In<Entity>;
    type Out = ();

    fn new(id: SystemId<In<Entity>>) -> Self {
        Self(id)
    }
}

impl RoomMaking {
    /// Makes room for the body of `entity` as `Navigation::make_room` says, when it stands, with
    /// `walkers`, the query of every living walker, and `inside`, a buffer it keeps.
    pub(super) fn make_room(
        In(entity): In<Entity>,
        world: &mut World,
        walkers: &mut QueryState<
            (Entity, &StableId, &Position, &Body),
            (With<MoveStep>, Without<Dead>),
        >,
        mut inside: Local<'_, Vec<Keyed>>,
    ) {
        let unit = world.entity(entity);
        let Some(&body) = unit.get::<Body>() else {
            return;
        };
        let shape = body.shape();
        if unit.contains::<MoveStep>() || shape == Shape::POINT {
            return;
        }
        let position = *unit.get::<Position>().expect("a unit has a place");
        let now = world.resource::<SimTick>().start();
        // The grid takes the body as it spawns, before a walker it overlaps finds its cell, and
        // before a spawn its producer sends at once finds its own (04-capabilities/navigation.md, A
        // spawn stands where it can walk).
        let run = world.resource::<StaticTrackingRun>().0;
        world
            .run_system(run)
            .expect("the static bodies' tracking runs on any world");
        inside.clear();
        inside.extend(
            walkers
                .iter(world)
                .filter(|&(_, _, &walker_pos, other)| {
                    let walker = Placed::new(walker_pos.get(), other.shape());
                    other.layer() == body.layer()
                        && Placed::new(position.get(), shape).overlaps(walker)
                })
                .map(|(entity, &id, ..)| Keyed { id, entity }),
        );
        if inside.is_empty() {
            return;
        }
        inside.sort_unstable_by_key(|keyed| keyed.id);
        for &Keyed { entity: walker, .. } in &*inside {
            let unit = world.entity(walker);
            let walker_pos = *unit.get::<Position>().expect("a walker has a place");
            let kind = Walker::walking(unit.get::<Body>());
            let grid = world.get_resource::<PathingGrid>();
            let planner = world.get_resource::<RoutePlanner>();
            let place = if let (Some(grid), Some(planner)) = (grid, planner) {
                let walkable = Walkable::of(grid.clearance(kind), world.resource::<BodyIndex>());
                planner
                    .stand_at(walkable, walker_pos.get())
                    .unwrap_or_else(|| walker_pos.get())
            } else {
                shape
                    .push_out(position.get(), walker_pos.get(), kind.radius)
                    .expect("the body overlaps the walker, so it pushes the walker out")
            };
            NavigationEffect::put(world, walker, place, now);
        }
    }
}
