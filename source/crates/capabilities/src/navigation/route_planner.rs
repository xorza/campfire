use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bevy_ecs::resource::Resource;
use campfire_sim::Position;

use crate::navigation::pathing_grid::Layer;
use crate::navigation::static_index::StaticIndex;
use crate::values::grid::Grid;
use crate::values::segment::Segment;

/// Plans long routes by A* on a layer of the pathing grid: eight neighbors, a straight step
/// costing 10 and a diagonal 14, and no diagonal past a blocked cell, so a route never cuts a
/// blocked corner. The estimate is the octile distance, which never overestimates those costs, so
/// the route is a cheapest one; among equal totals the cell with the lower estimate goes first,
/// then the lower cell number, so every run plans the same route. A goal a walker cannot stand on
/// gives way to the open cell nearest it, and a goal no route reaches to the reached cell nearest
/// it. The route then keeps only the cells where the straight line from the waypoint before would
/// overlap a static body, tested exactly. The buffers stay between routes, and a route touches
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

/// What planning a route cost, and whether it ends on the goal.
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
    /// `layer` among `statics`, into `waypoints` at the goal's height. The route ends on the goal
    /// when the walker may stand there, even in a cell whose center it may not, and a route
    /// reaches it. Otherwise it ends on the center of the open cell nearest the goal, or of the
    /// reached cell nearest it, ties to the cheaper, then to the lower number; with no open cell,
    /// it is empty. A start in a blocked cell may leave it for an open one.
    pub(crate) fn plan(
        &mut self,
        layer: Layer<'_>,
        statics: &StaticIndex,
        start: Position,
        goal: Position,
        waypoints: &mut Vec<Position>,
    ) -> Planned {
        waypoints.clear();
        let grid = layer.grid();
        let radius = layer.radius();
        let goal = grid.clamp(goal);
        let clear = !statics.blocks(Segment::new(goal, goal), radius);
        let target = if clear {
            Some(grid.nearest_cell(goal))
        } else {
            RoutePlanner::nearest_open(layer, goal)
        };
        let Some(target) = target else {
            return Planned {
                cost: 0,
                expanded: 0,
                reached: false,
            };
        };
        let from = grid.nearest_cell(start);
        self.routes += 1;
        let (seen, done) = (2 * self.routes, 2 * self.routes + 1);
        self.open.clear();
        self.marks[from] = seen;
        self.costs[from] = 0;
        self.came_from[from] = RoutePlanner::number(from);
        let estimate = RoutePlanner::estimate(grid, from, target);
        self.open.push(Reverse(Open {
            total: estimate,
            estimate,
            cell: RoutePlanner::number(from),
        }));
        let mut expanded = 0;
        let mut nearest = from;
        let mut found = false;
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
            if at == target {
                found = true;
                break;
            }
            self.expand(layer, at, target, seen, done);
        }
        let end = if found { target } else { nearest };
        self.cells.clear();
        let mut at = end;
        self.cells.push(RoutePlanner::number(at));
        while at != from {
            at = self.came_from[at] as usize;
            self.cells.push(RoutePlanner::number(at));
        }
        let reached = found && clear;
        let y = goal.get().y;
        let last = if reached { goal } else { grid.center(end, y) };
        let point = |index: usize| match index {
            0 => last,
            _ => grid.center(self.cells[index] as usize, y),
        };
        let mut anchor = self.cells.len() - 1;
        let mut from_point = start;
        while anchor > 0 {
            let mut next = anchor - 1;
            while next > 0 && !statics.blocks(Segment::new(from_point, point(next - 1)), radius) {
                next -= 1;
            }
            from_point = point(next);
            waypoints.push(from_point);
            anchor = next;
        }
        if waypoints.is_empty() {
            waypoints.push(last);
        }
        Planned {
            cost: self.costs[end],
            expanded,
            reached,
        }
    }

    /// The open cell of `layer` whose center is nearest `goal`, ties to the lower number; `None`
    /// with none. Rings of cells around the goal's go outward until a ring's centers must lie
    /// farther than the nearest found: a center `k` rings out is at least `k − ½` cells off.
    fn nearest_open(layer: Layer<'_>, goal: Position) -> Option<usize> {
        let grid = layer.grid();
        let (columns, rows) = (grid.columns(), grid.rows());
        let middle = grid.nearest_cell(goal);
        let (column, row) = (middle % columns, middle / columns);
        let cell = u128::from(grid.cell().to_bits().unsigned_abs());
        let mut best: Option<(u128, usize)> = None;
        for ring in 0..columns.max(rows) {
            if let Some((distance, _)) = best
                && ring > 0
            {
                let least = (2 * ring as u128 - 1) * cell;
                if least * least > distance {
                    break;
                }
            }
            let rows_of = row.saturating_sub(ring)..=(row + ring).min(rows - 1);
            for z in rows_of {
                let edge = z.abs_diff(row) == ring;
                let low = column.saturating_sub(ring);
                let high = (column + ring).min(columns - 1);
                for x in low..=high {
                    if !edge && x.abs_diff(column) != ring {
                        continue;
                    }
                    let at = z * columns + x;
                    if !layer.open(at) {
                        continue;
                    }
                    let candidate = (grid.center_distance(at, goal), at);
                    if best.is_none_or(|best| candidate < best) {
                        best = Some(candidate);
                    }
                }
            }
        }
        best.map(|(_, at)| at)
    }

    /// Reaches the open neighbors of `at`, and `to` whether open or not, more cheaply than before,
    /// in a fixed order.
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
            let entered = x < columns && z < grid.rows() && z * columns + x == to;
            if !entered && !open(x, z) {
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

    fn number(cell: usize) -> u32 {
        u32::try_from(cell).expect("a grid has at most 2²² cells")
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::{Num, Vec3};
    use campfire_sim::IdAllocator;

    use super::*;
    use crate::navigation::pathing_grid::PathingGrid;
    use crate::navigation::static_index::StaticBody;
    use crate::values::bounds::Bounds;

    /// The point `(x, z)` in quarters of a meter.
    fn at(x: i64, z: i64) -> Position {
        let quarter = |value: i64| Num::from_bits(value << (Num::FRAC_BITS - 2));
        Position::new(Vec3::new(quarter(x), Num::ZERO, quarter(z))).unwrap()
    }

    /// 1 m cells from the origin, rows from z = 0, with a post of 0.25 m on the center of each
    /// `#`, for walkers of 0.25 m: a post blocks its own cell, whose center is on it, and not its
    /// neighbors', 1 m off; the walker's center keeps 0.5 m from it.
    fn walled(rows: &[&str]) -> (PathingGrid, StaticIndex) {
        let quarter = Num::from_bits(1 << (Num::FRAC_BITS - 2));
        let size = |count: usize| Num::from_int(i64::try_from(count).unwrap()).unwrap();
        let bounds = Bounds::new([Num::ZERO; 2], [size(rows[0].len()), size(rows.len())]);
        let mut grid =
            PathingGrid::new(Grid::new(Num::ONE, bounds.unwrap()).unwrap(), vec![quarter]);
        let mut ids = IdAllocator::default();
        let mut posts = Vec::new();
        for (row, line) in rows.iter().enumerate() {
            for (column, mark) in line.chars().enumerate() {
                if mark == '#' {
                    let center = |index: usize| i64::try_from(4 * index + 2).unwrap();
                    posts.push(StaticBody {
                        id: ids.allocate(),
                        at: at(center(column), center(row)),
                        radius: quarter,
                    });
                }
            }
        }
        let mut statics = StaticIndex::new(quarter);
        statics.update(&posts);
        grid.update(&statics);
        (grid, statics)
    }

    #[test]
    fn a_route_goes_through_the_gap_and_never_across_a_blocked_corner() {
        // A wall down column 3 over rows 0 to 2, then down column 4 over rows 3 and 4. Cells (3, 2)
        // and (4, 3) meet at a corner, which a diagonal step from (3, 3) to (4, 2) would cut: the
        // route must go round through row 5.
        let (grid, statics) = walled(&[
            "...#....", "...#....", "...#....", "....#...", "....#...", "........",
        ]);
        let layer = grid.layer(Num::from_bits(1 << (Num::FRAC_BITS - 2)));
        let mut planner = RoutePlanner::new(layer.grid());
        let mut waypoints = Vec::new();
        let (start, goal) = (at(2, 10), at(29, 11));
        let first = planner.plan(layer, &statics, start, goal, &mut waypoints);
        // From (0, 2) three diagonals to (3, 5), 42; a step to (4, 5), 10; a step to (5, 5), as a
        // diagonal to (5, 4) would pass the blocked (4, 4), 10; then diagonals to (6, 4) and
        // (7, 3), 28, and a step to (7, 2), 10: 100. Across the corner it would be (1, 3), (2, 3),
        // (3, 3) and (4, 2), 14 + 10 + 10 + 14, and 30 on to (7, 2): 78.
        assert_eq!(first.cost, 100);
        assert!(first.reached);
        // The route runs (4, 5), (5, 5), (6, 4), (7, 3), (7, 2): among the cells of total 100 the
        // one of lower estimate goes first, (5, 5) at 38, then (6, 4) at 24, then (7, 3) at 10.
        // From the start, (0.5, 2.5), the line to (4.5, 5.5), along (4, 3), passes the post at
        // (4.5, 4.5) |4·2 − 3·4| / 5 = 0.8 m off, and the line to (5.5, 5.5), along (5, 3),
        // |5·2 − 3·4| / √34 ≈ 0.34 m off, less than 0.5. From (4.5, 5.5) the line to the goal,
        // (7.25, 2.75), along (1, −1), passes that post √0.5 ≈ 0.71 m off, and the post at
        // (4.5, 3.5) √2 m off.
        assert_eq!(waypoints, [at(18, 22), goal]);

        // The same route from a planner that planned others, and from a new one.
        let mut other = Vec::new();
        planner.plan(layer, &statics, at(29, 1), at(1, 21), &mut other);
        let mut again = Vec::new();
        assert_eq!(
            planner.plan(layer, &statics, start, goal, &mut again),
            first
        );
        assert_eq!(again, waypoints);
        let mut fresh = Vec::new();
        assert_eq!(
            RoutePlanner::new(layer.grid()).plan(layer, &statics, start, goal, &mut fresh),
            first
        );
        assert_eq!(fresh, waypoints);

        // A goal on the post at (3.5, 1.5): the nearest open centers, (2.5, 1.5) and (4.5, 1.5),
        // are a meter off, and (2, 1) has the lower number. The search expands (0, 2), then (1, 1)
        // of total 14 + 10, before (1, 2) of 10 + 14, then (2, 1) of 24 + 0.
        let blocked = planner.plan(layer, &statics, start, at(14, 6), &mut waypoints);
        assert_eq!(
            blocked,
            Planned {
                cost: 24,
                expanded: 3,
                reached: false,
            }
        );
        assert_eq!(waypoints, [at(10, 6)]);

        // A goal at (3.5, 3.25) in the open cell (3, 3), past the corner of the wall: a
        // walker may stand there, 0.75 m from (3.5, 2.5) and 1.03 m from (4.5, 3.5).
        let corner = planner.plan(layer, &statics, start, at(14, 13), &mut waypoints);
        assert!(corner.reached);
        assert_eq!(waypoints, [at(14, 13)]);

        // A goal in a blocked cell that a walker may stand on: (3.5, 0.5) is a post, but (3.85,
        // 0.0) is clear of it by √(0.35² + 0.5²) ≈ 0.61 m, so the route ends on it.
        let edge = Position::new(Vec3::new(
            Num::from_bits((385 << Num::FRAC_BITS) / 100),
            Num::ZERO,
            Num::ZERO,
        ))
        .unwrap();
        let tucked = planner.plan(layer, &statics, start, edge, &mut waypoints);
        assert!(tucked.reached);
        assert_eq!(waypoints.last(), Some(&edge));

        // A goal in the start's cell: nothing to expand past it.
        let here = planner.plan(layer, &statics, start, at(3, 9), &mut waypoints);
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
