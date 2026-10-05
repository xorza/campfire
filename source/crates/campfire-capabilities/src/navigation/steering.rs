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
    /// The bodies of the units that walk, by layer, which a stuck walker searches for those it
    /// touches.
    walking: BodyGrid<Layer>,
    /// The units that block the walker that steers now.
    blockers: Vec<IndexedBody>,
    /// The short route of the walker that steers now.
    short: Vec<Position>,
}

/// A walker that may steer this tick: its unit, where it stands, its step, its kind, and whether
/// walkers keep it back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Steered {
    pub(crate) id: StableId,
    pub(crate) at: Position,
    pub(crate) step: Num,
    pub(crate) walker: Walker,
    pub(crate) stuck: bool,
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
    /// buckets of `statics`, and `walking`, those of the units that walk, into a grid.
    pub(crate) fn read(
        &mut self,
        statics: &BodyIndex,
        still: impl IntoIterator<Item = IndexedBody>,
        walking: impl IntoIterator<Item = Placed<Layer>>,
    ) {
        self.still.clear();
        self.still.extend(still);
        self.still.sort_unstable_by_key(|body| body.id);
        let standing = self.standing.get_or_insert_with(|| statics.sibling());
        standing.update(&self.still);
        self.walking.rebuild(walking);
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
        } = steered;
        let standing = self.standing.as_ref().expect("steering read the bodies");
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
            self.walking.visit_near(at, walker.radius + step, |other| {
                let reach = walker.radius + other.radius;
                let touching = other.at.within_ground(at, reach + step);
                if other.id == id || !touching || other.key != walker.layer {
                    return;
                }
                let shift = left * (reach / 2);
                let moved = Position::new(other.at.get() + shift);
                self.blockers.push(IndexedBody {
                    id: other.id,
                    at: moved.expect("a shift of a body's reach stays within the bound"),
                    radius: other.radius,
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
    use crate::navigation::terrain::Terrain;
    use crate::values::bounds::Bounds;
    use crate::values::grid::Grid;

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
            radius: quarter,
            layer: Layer::FIRST,
        };
        let mut steering = Steering::default();
        steering.read(&statics, [standing], []);
        let goal = at(64, 0);
        let mut route = Route::default();
        route.ask(goal, Tick::new(0));
        route.answer(&[goal], true);
        let steered = Steered {
            id,
            at: at(7, 0),
            step: quarter,
            walker,
            stuck: false,
        };
        steering.steer(&mut planner, &grid, &statics, steered, &route);
        assert_eq!(steering.blockers, [standing]);
    }
}
