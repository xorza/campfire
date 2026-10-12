use std::cmp::Ordering;

use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::{Keyed, Position, SimTick, StableId};

use crate::geometry::grid::Grid;
use crate::geometry::shape::Shape;
use crate::navigation::body_index::BodyIndex;
use crate::navigation::destination::Destination;
use crate::navigation::navigation_effect::NavigationEffect;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::route_planner::{RoutePlanner, Walkable};
use crate::navigation::static_changes::StaticChanges;
use crate::navigation::static_tracking::StaticTracking;
use crate::navigation::statics_dirty::StaticsDirty;
use crate::navigation::terrain::Terrain;
use crate::navigation::walker::Walker;
use crate::navigation::wall::Wall;
use crate::navigation::walls::Walls;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::move_step::MoveStep;

/// What the navigation module gives a world, for its own crate.
pub(crate) trait NavigationWorldCrate {
    /// Sets the destination of the unit of `entity`, which walks, to `target`, leaving one that
    /// does not change untouched.
    fn go_to(&mut self, entity: Entity, target: Option<Position>);

    /// Gives the match the map's pathing grid on the cells of `grid`, those `walls` block, for the
    /// kinds of `walkers`, a static index for the widest of them, a planner of routes on the
    /// grid, and the walls for placements to test; the static bodies fill the grid and the index
    /// from the first tick on.
    fn load_pathing(&mut self, grid: Grid, walls: &[Wall], walkers: Vec<Walker>);

    /// Makes room for the box of `entity`, which just spawned: each walker of its layer whose
    /// body the box overlaps goes at once to the nearest cell its walker may stand in, by stable
    /// id, as a teleport places a unit but with no disjoint. The static index and the pathing
    /// grid take the box first, so the cell is clear of every static body, and no push of
    /// collision can hold the walker between two. With no pathing grid, the box pushes it out
    /// as collision would.
    fn make_room(&mut self, entity: Entity);

    /// The center of the cell nearest `point` among those `walker` may stand in, at `point`'s
    /// height, as a teleport finds its cell, with the static bodies as the tick began; `None`
    /// with no pathing grid, or on a grid with no such cell.
    fn open_cell(&self, walker: Walker, point: Position) -> Option<Position>;
}

impl NavigationWorldCrate for World {
    fn go_to(&mut self, entity: Entity, target: Option<Position>) {
        if let Some(mut destination) = self.get_mut::<Destination>(entity) {
            destination.set_if_neq(Destination::to(target));
        }
    }

    fn load_pathing(&mut self, grid: Grid, walls: &[Wall], walkers: Vec<Walker>) {
        debug_assert!(
            self.contains_resource::<StaticChanges>(),
            "a map's pathing grid is navigation's, which the load checked the mode declares"
        );
        let terrain = Terrain::new(&grid, walls);
        let widest = walkers.iter().map(|walker| walker.radius).max();
        self.insert_resource(BodyIndex::new(widest.unwrap_or(Num::ZERO)));
        self.insert_resource(RoutePlanner::new(&grid));
        self.insert_resource(PathingGrid::new(grid, walkers, &terrain));
        self.insert_resource(Walls::new(walls));
        self.resource_mut::<StaticsDirty>().set();
    }

    fn make_room(&mut self, entity: Entity) {
        self.run_system_cached(StaticTracking::track_static_bodies)
            .expect("the static bodies' tracking runs on any world");
        let unit = self.entity(entity);
        let pos = *unit.get::<Position>().expect("a unit has a place");
        let body = *unit.get::<Body>().expect("a box makes room");
        let Shape::Box(boxed) = body.shape() else {
            return;
        };
        let now = self.resource::<SimTick>().start();
        let mut walkers =
            self.query_filtered::<(Entity, &StableId, &Position, &Body), (With<MoveStep>, Without<Dead>)>();
        let mut inside: Vec<Keyed> = walkers
            .iter(self)
            .filter(|&(_, _, &walker_pos, other)| {
                let radius = Walker::walking(Some(other)).radius;
                other.layer() == body.layer()
                    && boxed.nearest(pos, walker_pos, radius) == Ordering::Less
            })
            .map(|(entity, &id, ..)| Keyed { id, entity })
            .collect();
        inside.sort_unstable_by_key(|keyed| keyed.id);
        for Keyed { entity: walker, .. } in inside {
            let unit = self.entity(walker);
            let walker_pos = *unit.get::<Position>().expect("a walker has a place");
            let kind = Walker::walking(unit.get::<Body>());
            let grid = self.get_resource::<PathingGrid>();
            let planner = self.get_resource::<RoutePlanner>();
            let place = if let (Some(grid), Some(planner)) = (grid, planner) {
                let walkable = Walkable::of(grid.clearance(kind), self.resource::<BodyIndex>());
                planner.stand_at(walkable, walker_pos).unwrap_or(walker_pos)
            } else {
                let out = boxed
                    .push_out(pos, walker_pos.get(), kind.radius)
                    .expect("the box overlaps the walker, so it pushes the walker out");
                Position::new(out).expect("a push stays near the bounds")
            };
            NavigationEffect::put(self, walker, place, now);
        }
    }

    fn open_cell(&self, walker: Walker, point: Position) -> Option<Position> {
        let grid = self.get_resource::<PathingGrid>()?;
        let planner = self.get_resource::<RoutePlanner>()?;
        let clearance = grid.serving(walker)?;
        let walkable = Walkable::of(clearance, self.resource::<BodyIndex>());
        let cell = planner.nearest_open(walkable, point, &mut 0)?;
        Some(clearance.grid().center(cell, point.get().y))
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_sim::{Position, StableId};

    use crate::geometry::bounds::Bounds;
    use crate::geometry::grid::Grid;
    use crate::navigation::Navigation;
    use crate::navigation::destination::Destination;
    use crate::navigation::navigation_world::NavigationWorldCrate;
    use crate::navigation::route::Route;
    use crate::navigation::walker::Walker;
    use crate::navigation::wall::Wall;
    use crate::release::release_world::test_world::ReleaseWorldTest;
    use crate::units::body::Body;
    use crate::units::move_step::MoveStep;
    use crate::units::team::Team;

    /// What the navigation module gives a world for its own tests.
    pub(crate) trait NavigationWorldTest {
        /// Loads a pathing grid of `cell` cells from `min` to `max` for `walkers`.
        fn load_open_pathing(
            &mut self,
            cell: Num,
            min: [i64; 2],
            max: [i64; 2],
            walkers: Vec<Walker>,
        );

        /// Loads a pathing grid as `load_pathing` does, with `walls`.
        fn load_walled(
            &mut self,
            cell: Num,
            min: [i64; 2],
            max: [i64; 2],
            walkers: Vec<Walker>,
            walls: &[Wall],
        );

        /// A unit at `pos` walking a meter a tick to `to`.
        fn unit(&mut self, pos: Position, to: Option<Position>) -> StableId;

        /// A unit at `pos` with a body of `radius`, walking `step` a tick to `to`, or one that does not
        /// walk when `step` is `None`.
        fn body(
            &mut self,
            pos: Position,
            to: Option<Position>,
            step: Option<Num>,
            radius: Num,
        ) -> StableId;

        /// A unit at `pos` with `body`, walking `step` a tick to `to`, or one that does not walk when
        /// `step` is `None`.
        fn body_on(
            &mut self,
            pos: Position,
            to: Option<Position>,
            step: Option<Num>,
            body: Body,
        ) -> StableId;

        fn get_route(&self, id: StableId) -> &Route;
    }

    impl NavigationWorldTest for World {
        fn load_open_pathing(
            &mut self,
            cell: Num,
            min: [i64; 2],
            max: [i64; 2],
            walkers: Vec<Walker>,
        ) {
            self.load_walled(cell, min, max, walkers, &[]);
        }

        fn load_walled(
            &mut self,
            cell: Num,
            min: [i64; 2],
            max: [i64; 2],
            walkers: Vec<Walker>,
            walls: &[Wall],
        ) {
            let bounds = Bounds::new(min.map(Num::int), max.map(Num::int)).unwrap();
            let grid = Grid::new(cell, bounds).unwrap();
            self.load_pathing(grid, walls, walkers);
        }

        fn unit(&mut self, pos: Position, to: Option<Position>) -> StableId {
            let walker = Navigation::walker(MoveStep::new(Num::ONE).unwrap());
            let id = self.spawn_unit(pos, Team::new(0), walker);
            *self.part_mut::<Destination>(id) = Destination::to(to);
            id
        }

        fn body(
            &mut self,
            pos: Position,
            to: Option<Position>,
            step: Option<Num>,
            radius: Num,
        ) -> StableId {
            self.body_on(pos, to, step, Body::new(radius).unwrap())
        }

        fn body_on(
            &mut self,
            pos: Position,
            to: Option<Position>,
            step: Option<Num>,
            body: Body,
        ) -> StableId {
            let id = self.spawn_unit(pos, Team::new(0), body);
            if let Some(step) = step {
                self.insert_parts(id, Navigation::walker(MoveStep::new(step).unwrap()));
                *self.part_mut::<Destination>(id) = Destination::to(to);
            }
            id
        }

        fn get_route(&self, id: StableId) -> &Route {
            let entity = self.unit_entity(id);
            self.get::<Route>(entity).unwrap()
        }
    }
}
