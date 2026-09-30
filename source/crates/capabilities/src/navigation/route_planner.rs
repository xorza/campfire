use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

use bevy_ecs::resource::Resource;
use campfire_sim::Position;

use crate::navigation::pathing_grid::Layer;
use crate::values::grid::Grid;

/// Plans long routes by A* on a layer of the pathing grid: eight neighbors, a straight step
/// costing 10 and a diagonal 14, and no diagonal past a blocked cell, so a route never cuts a
/// blocked corner. The estimate is the octile distance, which never overestimates those costs, so
/// the route is a cheapest one; among equal totals the cell with the lower estimate goes first,
/// then the lower cell number, so every run plans the same route. A goal no route reaches gives a
/// route to the reached cell nearest it. The route then keeps only the cells where the straight
/// line from the waypoint before is blocked. The buffers stay between routes, and a route touches
/// only the cells it reaches.
#[derive(Resource, Debug)]
pub(crate) struct RoutePlanner {
    /// The route each cell was last reached in, as twice its number, plus one once expanded.
    marks: Vec<u64>,
    /// Each reached cell's cost from the start, and the cell it was reached from.
    costs: Vec<u32>,
    came_from: Vec<u32>,
    /// The number of routes planned.
    routes: u64,
    open: BinaryHeap<Reverse<Open>>,
    /// The cells of the route, the last first.
    cells: Vec<u32>,
}

/// A cell to expand: by its total, cost plus estimate, then by its estimate, then by its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Open {
    total: u32,
    estimate: u32,
    cell: u32,
}

/// What planning a route cost, and whether it reached the goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Planned {
    /// The cost of the route's cells, 10 a straight step.
    pub(crate) cost: u32,
    /// The cells the search expanded.
    pub(crate) expanded: u32,
    pub(crate) reached: bool,
}

impl RoutePlanner {
    const STRAIGHT: u32 = 10;
    const DIAGONAL: u32 = 14;

    /// A planner for routes on `grid`'s cells.
    pub(crate) fn new(grid: &Grid) -> RoutePlanner {
        let cells = grid.cells();
        RoutePlanner {
            marks: vec![0; cells],
            costs: vec![0; cells],
            came_from: vec![0; cells],
            routes: 0,
            open: BinaryHeap::new(),
            cells: Vec::new(),
        }
    }

    /// Plans the route from `start` to `goal`, both taken to the nearest point of the bounds, on
    /// `layer`, into `waypoints` at the goal's height. The last waypoint is the goal when a route
    /// reaches its cell; otherwise the center of the reached cell nearest the goal, ties to the
    /// cheaper, then to the lower number. A start in a blocked cell may leave it for an open one.
    pub(crate) fn plan(
        &mut self,
        layer: Layer<'_>,
        start: Position,
        goal: Position,
        waypoints: &mut Vec<Position>,
    ) -> Planned {
        let grid = layer.grid();
        let goal = grid.clamp(goal);
        let from = grid.nearest_cell(start);
        let to = grid.nearest_cell(goal);
        self.routes += 1;
        let (seen, done) = (2 * self.routes, 2 * self.routes + 1);
        self.open.clear();
        self.marks[from] = seen;
        self.costs[from] = 0;
        self.came_from[from] = RoutePlanner::number(from);
        let estimate = RoutePlanner::estimate(grid, from, to);
        self.open.push(Reverse(Open {
            total: estimate,
            estimate,
            cell: RoutePlanner::number(from),
        }));
        let mut expanded = 0;
        let mut nearest = from;
        let mut reached = false;
        while let Some(Reverse(Open { cell, .. })) = self.open.pop() {
            let at = cell as usize;
            if self.marks[at] == done {
                continue;
            }
            self.marks[at] = done;
            expanded += 1;
            let key = |cell: usize| (grid.center_distance(cell, goal), self.costs[cell], cell);
            if key(at) < key(nearest) {
                nearest = at;
            }
            if at == to {
                reached = true;
                break;
            }
            self.expand(layer, at, to, seen, done);
        }
        let end = if reached { to } else { nearest };
        self.cells.clear();
        let mut at = end;
        self.cells.push(RoutePlanner::number(at));
        while at != from {
            at = self.came_from[at] as usize;
            self.cells.push(RoutePlanner::number(at));
        }
        let y = goal.get().y;
        waypoints.clear();
        let mut anchor = self.cells.len() - 1;
        while anchor > 0 {
            let mut next = anchor - 1;
            while next > 0 && RoutePlanner::visible(layer, self.cells[anchor], self.cells[next - 1])
            {
                next -= 1;
            }
            if next > 0 {
                waypoints.push(grid.center(self.cells[next] as usize, y));
            }
            anchor = next;
        }
        waypoints.push(if reached { goal } else { grid.center(end, y) });
        Planned {
            cost: self.costs[end],
            expanded,
            reached,
        }
    }

    /// Reaches the open neighbors of `at` more cheaply than before, in a fixed order.
    fn expand(&mut self, layer: Layer<'_>, at: usize, to: usize, seen: u64, done: u64) {
        let grid = layer.grid();
        let columns = grid.columns();
        let (column, row) = (at % columns, at / columns);
        let open = |column: usize, row: usize| {
            column < columns && row < grid.rows() && layer.open(row * columns + column)
        };
        for (dx, dz) in [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ] {
            let (x, z) = (column.wrapping_add_signed(dx), row.wrapping_add_signed(dz));
            if !open(x, z) {
                continue;
            }
            let step = if dx != 0 && dz != 0 {
                if !open(x, row) || !open(column, z) {
                    continue;
                }
                RoutePlanner::DIAGONAL
            } else {
                RoutePlanner::STRAIGHT
            };
            let next = z * columns + x;
            let cost = self.costs[at] + step;
            if self.marks[next] == done || (self.marks[next] == seen && self.costs[next] <= cost) {
                continue;
            }
            self.marks[next] = seen;
            self.costs[next] = cost;
            self.came_from[next] = RoutePlanner::number(at);
            let estimate = RoutePlanner::estimate(grid, next, to);
            self.open.push(Reverse(Open {
                total: cost + estimate,
                estimate,
                cell: RoutePlanner::number(next),
            }));
        }
    }

    /// The octile distance from cell `a` to cell `b`: diagonal steps while both axes differ,
    /// then straight ones.
    fn estimate(grid: &Grid, a: usize, b: usize) -> u32 {
        let columns = grid.columns();
        let dx = (a % columns).abs_diff(b % columns);
        let dz = (a / columns).abs_diff(b / columns);
        let (long, short) = (dx.max(dz), dx.min(dz));
        RoutePlanner::STRAIGHT * RoutePlanner::number(long - short)
            + RoutePlanner::DIAGONAL * RoutePlanner::number(short)
    }

    /// Whether the straight line from the center of cell `a` to that of cell `b` crosses only
    /// open cells, `a` aside; where it passes through a corner, the two cells beside it are open
    /// too, as for a diagonal step.
    fn visible(layer: Layer<'_>, a: u32, b: u32) -> bool {
        let columns = layer.grid().columns();
        let (a, b) = (a as usize, b as usize);
        let (mut x, mut z) = (a % columns, a / columns);
        let (nx, nz) = ((b % columns).abs_diff(x), (b / columns).abs_diff(z));
        let sx: isize = if b % columns < x { -1 } else { 1 };
        let sz: isize = if b / columns < z { -1 } else { 1 };
        let open = |x: usize, z: usize| layer.open(z * columns + x);
        let (mut ix, mut iz) = (0, 0);
        // The line crosses its (ix + 1)th column line at (2ix + 1) / 2nx of its length, and its
        // (iz + 1)th row line at (2iz + 1) / 2nz.
        while ix < nx || iz < nz {
            let order = if ix == nx {
                Ordering::Greater
            } else if iz == nz {
                Ordering::Less
            } else {
                ((2 * ix + 1) * nz).cmp(&((2 * iz + 1) * nx))
            };
            match order {
                Ordering::Equal => {
                    let (across, along) = (x.wrapping_add_signed(sx), z.wrapping_add_signed(sz));
                    if !open(across, z) || !open(x, along) {
                        return false;
                    }
                    (x, z) = (across, along);
                    ix += 1;
                    iz += 1;
                }
                Ordering::Less => {
                    x = x.wrapping_add_signed(sx);
                    ix += 1;
                }
                Ordering::Greater => {
                    z = z.wrapping_add_signed(sz);
                    iz += 1;
                }
            }
            if !open(x, z) {
                return false;
            }
        }
        true
    }

    fn number(cell: usize) -> u32 {
        u32::try_from(cell).expect("a grid has at most 2²² cells")
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::{Num, Vec3};

    use super::*;
    use crate::navigation::pathing_grid::PathingGrid;

    /// The point `(x, z)` in quarters of a meter.
    fn at(x: i64, z: i64) -> Position {
        let quarter = |value: i64| Num::from_bits(value << (Num::FRAC_BITS - 2));
        Position::new(Vec3::new(quarter(x), Num::ZERO, quarter(z))).unwrap()
    }

    #[test]
    fn a_route_goes_through_the_gap_and_never_across_a_blocked_corner() {
        // A wall down column 3 over rows 0 to 2, then down column 4 over rows 3 and 4. Cells (3, 2)
        // and (4, 3) meet at a corner, which a diagonal step from (3, 3) to (4, 2) would cut: the
        // route must go round through row 5.
        let grid = PathingGrid::from_picture(
            Num::ZERO,
            &[
                "...#....", "...#....", "...#....", "....#...", "....#...", "........",
            ],
        );
        let layer = grid.layer(Num::ZERO);
        let mut planner = RoutePlanner::new(layer.grid());
        let mut waypoints = Vec::new();
        let (start, goal) = (at(2, 10), at(29, 11));
        let first = planner.plan(layer, start, goal, &mut waypoints);
        // From (0, 2) three diagonals to (3, 5), 42; a step to (4, 5), 10; a step to (5, 5), as a
        // diagonal to (5, 4) would pass the blocked (4, 4), 10; then diagonals to (6, 4) and
        // (7, 3), 28, and a step to (7, 2), 10: 100. Across the corner it would be (1, 3), (2, 3),
        // (3, 3) and (4, 2), 14 + 10 + 10 + 14, and 30 on to (7, 2): 78.
        assert_eq!(first.cost, 100);
        assert!(first.reached);
        // Among the cells of total 100 the one of lower estimate goes first: (5, 5) at 38, then
        // (6, 4) at 24, then (7, 3) at 10, which reaches the goal; so the route runs (4, 5),
        // (5, 5), (6, 4), (7, 3), (7, 2). The start sees (4, 5), not (5, 5), whose line passes
        // (4, 4); (4, 5) sees (7, 3), not (7, 2), whose line is a diagonal past (4, 4).
        assert_eq!(waypoints, [at(18, 22), at(30, 14), goal]);

        // The same route from a planner that planned others, and from a new one.
        let mut other = Vec::new();
        planner.plan(layer, at(29, 1), at(1, 21), &mut other);
        let mut again = Vec::new();
        assert_eq!(planner.plan(layer, start, goal, &mut again), first);
        assert_eq!(again, waypoints);
        let mut fresh = Vec::new();
        assert_eq!(
            RoutePlanner::new(layer.grid()).plan(layer, start, goal, &mut fresh),
            first
        );
        assert_eq!(fresh, waypoints);

        // A goal in the wall, at (3.5, 1.5): the nearest reached centers, (2.5, 1.5) and
        // (4.5, 1.5), are a meter off; (2, 1) is the cheaper, two steps of 10 and 14. The search
        // expands every cell it reaches: all 48 but the wall's 5.
        let blocked = planner.plan(layer, start, at(14, 6), &mut waypoints);
        assert_eq!(
            blocked,
            Planned {
                cost: 24,
                expanded: 43,
                reached: false,
            }
        );
        assert_eq!(waypoints, [at(10, 6)]);

        // A goal in the start's cell: nothing to expand past it.
        let here = planner.plan(layer, start, at(3, 9), &mut waypoints);
        assert_eq!(
            here,
            Planned {
                cost: 0,
                expanded: 1,
                reached: true,
            }
        );
        assert_eq!(waypoints, [at(3, 9)]);
    }
}
