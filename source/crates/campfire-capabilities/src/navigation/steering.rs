use campfire_math::{Num, Vec3};
use campfire_sim::{Position, StableId};

use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::route::Route;
use crate::navigation::route_planner::{RoutePlanner, Short, Walkable, Window};
use crate::navigation::segment::Segment;
use crate::navigation::walker::Walker;
use crate::units::body_grid::{BodyGrid, Placed};
use crate::units::layer::Layer;

/// Steers walkers round the units in their way, by short routes in a window of cells around each.
/// It keeps between ticks the index of the units that stand, made on the first tick with the
/// static index's buckets, the grid of those that walk, and the buffers each tick refills, so a
/// tick allocates nothing once they have grown.
#[derive(Debug, Default)]
pub(crate) struct Steering {
    standing: Option<BodyIndex>,
    /// The bodies of the units that stand, by stable id.
    still: Vec<IndexedBody>,
    /// The units in a gather loop, by stable id, which a walker in one passes through.
    gatherers: Vec<StableId>,
    /// The index of the units that stand and do not gather, for a walker that gathers; none
    /// until a unit gathers.
    apart: Option<BodyIndex>,
    still_apart: Vec<IndexedBody>,
    /// The bodies of the units that walk, by layer, which a stuck walker searches for those it
    /// touches.
    walking: BodyGrid<Layer>,
    /// The units that block the walker that steers now.
    blockers: Vec<IndexedBody>,
    /// The short route of the walker that steers now.
    short: Vec<Position>,
}

/// A walker that may steer this tick: its unit, where it stands, its step, its kind, whether
/// walkers keep it back, and whether it gathers, which passes through the others that do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Steered {
    pub(crate) id: StableId,
    pub(crate) at: Position,
    pub(crate) step: Num,
    pub(crate) walker: Walker,
    pub(crate) stuck: bool,
    pub(crate) gathering: bool,
}

/// A short route a walker takes: in place of the next `skipped` waypoints of its route, and
/// whether it ends on the goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Detour {
    pub(crate) skipped: usize,
    pub(crate) reached: bool,
}

impl Steering {
    /// The cells a short route's window reaches from the walker's along each axis: room to go
    /// round a unit some cells wide on either side, in a search of at most 17 × 17 cells.
    pub(crate) const WINDOW: usize = 8;
    /// How long a walker moves less than half a step a tick before it marks the walkers it
    /// touches: long enough that a walker yielding for a moment keeps its way.
    pub(crate) const STUCK_MS: u64 = 150;

    /// Reads this tick's bodies: `still`, those of the units that stand, into an index with the
    /// buckets of `statics`, and `walking`, those of the units that walk, into a grid; and
    /// `gatherers`, the units in a gather loop, whose standing bodies a second index leaves out.
    pub(crate) fn read(
        &mut self,
        statics: &BodyIndex,
        still: impl IntoIterator<Item = IndexedBody>,
        walking: impl IntoIterator<Item = Placed<Layer>>,
        gatherers: impl IntoIterator<Item = StableId>,
    ) {
        self.still.clear();
        self.still.extend(still);
        self.still.sort_unstable_by_key(|body| body.id);
        let standing = self.standing.get_or_insert_with(|| statics.sibling());
        standing.update(&self.still);
        self.walking.rebuild(walking);
        self.gatherers.clear();
        self.gatherers.extend(gatherers);
        self.gatherers.sort_unstable();
        if self.gatherers.is_empty() {
            return;
        }
        let gatherers = &self.gatherers;
        self.still_apart.clear();
        self.still_apart.extend(
            self.still
                .iter()
                .filter(|body| gatherers.binary_search(&body.id).is_err()),
        );
        let apart = self.apart.get_or_insert_with(|| statics.sibling());
        apart.update(&self.still_apart);
    }

    /// Steers `steered` along `route`, which has a waypoint ahead, on `grid` round `statics`: the
    /// detour it takes, by `short`, if any. One whose next stretch, as far as its window reaches,
    /// would overlap a unit of its layer that stands plans a short route in the window, with the
    /// units that stand there as blockers; so does one that is stuck, which marks as blockers
    /// the walkers it touches too. The short route goes to the last waypoint within the window,
    /// or to where the way leaves it, and the walker goes on from there; one that cannot end
    /// there keeps its route. One that `planner` has no work left for this tick keeps its route.
    pub(crate) fn steer(
        &mut self,
        planner: &mut RoutePlanner,
        grid: &PathingGrid,
        statics: &BodyIndex,
        steered: Steered,
        route: &Route,
    ) -> Option<Detour> {
        let Steered {
            id,
            at,
            step,
            walker,
            stuck,
            gathering,
        } = steered;
        let standing = match (gathering && !self.gatherers.is_empty(), &self.apart) {
            (true, Some(apart)) => apart,
            _ => self.standing.as_ref().expect("steering read the bodies"),
        };
        let window_cells = i64::try_from(Steering::WINDOW).expect("a small window");
        let reach = grid.cell() * window_cells;
        let look = at.get().step_toward(route.ahead()[0].get(), reach);
        let look = Position::new(look).expect("a step ends between two points within the bound");
        if (!stuck && !standing.blocks(Segment::new(at, look), walker)) || planner.spent() {
            return None;
        }
        let clearance = grid.clearance(walker);
        let cells = clearance.grid();
        let window = Window::around(cells, cells.nearest_cell(at), Steering::WINDOW);
        let inside = |pos: &Position| {
            let cell = cells.nearest_cell(*pos);
            window.contains(cell % cells.columns(), cell / cells.columns())
        };
        let ahead = route.ahead();
        let within = ahead.iter().take_while(|pos| inside(pos)).count();
        let (goal, skipped) = match within {
            0 => (look, 0),
            _ => (ahead[within - 1], within),
        };
        let last = skipped == ahead.len();
        self.blockers.clear();
        // The window's centers lie up to its half and half a cell from `at` along each axis, and
        // a body blocks one it comes closer to than the two radii.
        let half_cell = grid.cell() / 2;
        let near = reach + half_cell + walker.radius;
        standing.near(walker.layer, at.get(), near, |body| {
            self.blockers.push(*body);
        });
        let way = at.ground_offset(goal);
        let left = Vec3::new(-way.z, Num::ZERO, way.x).normalized();
        if let (true, Some(left)) = (stuck, left) {
            // A walker that keeps this one back counts as standing half their reach to this one's
            // left, so this one goes round it on its right; two that meet head on so pass on
            // opposite sides, whatever the cells make of their sides.
            let gatherers = &self.gatherers;
            self.walking.visit_near(at, walker.radius + step, |other| {
                let reach = walker.radius + other.shape.bound();
                let touching = other.at.within_ground(at, reach + step);
                let passes = gathering && gatherers.binary_search(&other.id).is_ok();
                if other.id == id || !touching || other.key != walker.layer || passes {
                    return;
                }
                let shift = left * (reach / 2);
                let moved = Position::new(other.at.get() + shift);
                self.blockers.push(IndexedBody {
                    id: other.id,
                    at: moved.expect("a shift of a body's reach stays within the bound"),
                    shape: other.shape,
                    layer: other.key,
                });
            });
        }
        let walkable = Walkable {
            clearance,
            statics,
            short: Some(Short {
                window,
                blockers: &self.blockers,
            }),
        };
        let outcome = planner.plan(walkable, at, goal, &mut self.short);
        // A plan with no cell to stand in would splice nothing in, and the walker would drop its
        // destination; it keeps its route, and steers again.
        let detour = Detour {
            skipped,
            reached: outcome.reached,
        };
        (!self.short.is_empty() && (outcome.reached || last)).then_some(detour)
    }

    /// The short route of the last detour.
    pub(crate) fn short(&self) -> &[Position] {
        &self.short
    }
}

#[cfg(test)]
mod tests {
    use campfire_common::Tick;
    use campfire_sim::IdAllocator;

    use super::*;
    use crate::geometry::bounds::Bounds;
    use crate::geometry::grid::Grid;
    use crate::geometry::shape::Shape;
    use crate::navigation::terrain::Terrain;

    /// The point `(x, z)` in eighths of a meter.
    fn at(x: i64, z: i64) -> Position {
        let eighth = |value: i64| Num::from_bits(value << (Num::FRAC_BITS - 3));
        Position::new(Vec3::new(eighth(x), Num::ZERO, eighth(z))).unwrap()
    }

    #[test]
    fn a_wide_walker_counts_every_unit_that_blocks_a_cell_of_its_window() {
        // Cells of 0.25 m, so a window reaches 8 cells, 2 m, each way. A walker of 2.5 m at
        // (0.875, 0), the center of its cell, bound for (8, 0), looks 2 m ahead, to (2.875, 0);
        // a unit of 0.25 m that stands at (5.375, 0) comes 2.5 m from there, within the two
        // radii, 2.75 m, so the walker steers, and the unit blocks the window's last centers. It
        // is a blocker, which a search of twice the window, to x = 4.875, would miss: the index's
        // buckets are 5 m, twice the walker, and the unit's box, from x = 5.125, is in the second.
        let quarter = Num::QUARTER;
        let wide = Num::from_int(10).unwrap() * quarter;
        let walker = Walker {
            layer: Layer::FIRST,
            radius: wide,
        };
        let bounds = Bounds::new(
            [Num::from_int(-8).unwrap(); 2],
            [Num::from_int(8).unwrap(); 2],
        );
        let cells = Grid::new(quarter, bounds.unwrap()).unwrap();
        let mut planner = RoutePlanner::new(&cells);
        let grid = PathingGrid::new(cells, vec![walker], &Terrain::default());
        let statics = BodyIndex::new(wide);
        let mut ids = IdAllocator::default();
        let (id, unit) = (ids.allocate(), ids.allocate());
        let standing = IndexedBody {
            id: unit,
            at: at(43, 0),
            shape: Shape::Circle(quarter),
            layer: Layer::FIRST,
        };
        let mut steering = Steering::default();
        steering.read(&statics, [standing], [], []);
        let goal = at(64, 0);
        let mut route = Route::default();
        route.ask(goal, Tick::new(0), None);
        route.answer(&[goal], true);
        let steered = Steered {
            id,
            at: at(7, 0),
            step: quarter,
            walker,
            stuck: false,
            gathering: false,
        };
        steering.steer(&mut planner, &grid, &statics, steered, &route);
        assert_eq!(steering.blockers, [standing]);
    }

    #[test]
    fn a_walker_that_gathers_steers_round_no_unit_that_gathers() {
        // A unit of a quarter meter stands at (1, 0), in the way of a walker of a quarter meter
        // at the origin bound for (6, 0), which looks 2 m ahead: a walker that gathers, past a
        // unit that gathers too, sees no blocker and keeps its route; one that does not gather
        // steers round it, by a short route to (2, 0).
        let quarter = Num::QUARTER;
        let walker = Walker {
            layer: Layer::FIRST,
            radius: quarter,
        };
        let bounds = Bounds::new([Num::int(-8); 2], [Num::int(8); 2]);
        let cells = Grid::new(quarter, bounds.unwrap()).unwrap();
        let grid = PathingGrid::new(cells, vec![walker], &Terrain::default());
        let statics = BodyIndex::new(quarter);
        let mut ids = IdAllocator::default();
        let (id, unit) = (ids.allocate(), ids.allocate());
        let standing = IndexedBody {
            id: unit,
            at: at(8, 0),
            shape: Shape::Circle(quarter),
            layer: Layer::FIRST,
        };
        let goal = at(48, 0);
        let mut route = Route::default();
        route.ask(goal, Tick::new(0), None);
        route.answer(&[goal], true);
        let detours = [true, false].map(|gathering| {
            let mut planner = RoutePlanner::new(&cells);
            let mut steering = Steering::default();
            steering.read(&statics, [standing], [], [unit]);
            let steered = Steered {
                id,
                at: at(0, 0),
                step: quarter,
                walker,
                stuck: false,
                gathering,
            };
            steering
                .steer(&mut planner, &grid, &statics, steered, &route)
                .is_some()
        });
        assert_eq!(detours, [false, true]);
    }
}
