#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{Position, StableId};

    use crate::geometry::bounds::Bounds;
    use crate::geometry::grid::Grid;
    use crate::navigation::Navigation;
    use crate::navigation::destination::Destination;
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
            Navigation::load_pathing(self, grid, walls, walkers);
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
