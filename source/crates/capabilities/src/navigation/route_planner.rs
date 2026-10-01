use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bevy_ecs::resource::Resource;
use campfire_sim::Position;

use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::pathing_grid::Clearance;
use crate::navigation::regions::Candidate;
use crate::values::grid::Grid;
use crate::values::segment::Segment;

/// Plans routes by A* on a clearance of the pathing grid: eight neighbors, a straight step costing 10
/// and a diagonal 14, and no diagonal past a blocked cell, so a route never cuts a blocked corner.
/// The estimate is the octile distance, which never overestimates those costs, so the route is a
/// cheapest one; among equal totals the cell with the lower estimate goes first, then the lower
/// cell number, so every run plans the same route. A goal a walker cannot stand on gives way to
/// the open cell nearest it, and a goal no route reaches to the nearest cell the walker reaches,
/// which the clearance's regions find before the search; a short route, which they do not serve,
/// ends on the reached cell nearest it. The route then keeps only the cells where the straight line from the
/// waypoint before would overlap a body, tested exactly. The buffers stay between routes, and a
/// route touches only the cells it reaches.
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
    /// A short route's window cells its blockers block, one bit a cell, row by row.
    overlay: Vec<u64>,
    /// The regions the nearest reachable cell to a goal may lie in.
    candidates: Vec<Candidate>,
}

/// Where a route may go: the clearance of its walker with the static bodies it was marked from,
/// and on a short route, its window and blockers, all of the walker's layer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Walkable<'a> {
    pub(crate) clearance: Clearance<'a>,
    pub(crate) statics: &'a BodyIndex,
    pub(crate) short: Option<Short<'a>>,
}

/// A short route's window of cells, which it stays in, and the units that block it there.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Short<'a> {
    pub(crate) window: Window,
    pub(crate) blockers: &'a [IndexedBody],
}

/// A square of cells, columns and rows from `low` to `high`, both in, `[column, row]` each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Window {
    low: [usize; 2],
    high: [usize; 2],
}

/// A cell to expand: by its total, cost plus estimate, then by its estimate, then by its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Open {
    total: u32,
    estimate: u32,
    cell: u32,
}

/// One search's target and marks.
#[derive(Debug, Clone, Copy)]
struct Search {
    to: usize,
    seen: u64,
    done: u64,
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

impl Window {
    /// The cells up to `half` cells from `cell` along each axis, within `grid`.
    pub(crate) fn around(grid: &Grid, cell: usize, half: usize) -> Window {
        let (column, row) = (cell % grid.columns(), cell / grid.columns());
        Window {
            low: [column.saturating_sub(half), row.saturating_sub(half)],
            high: [
                (column + half).min(grid.columns() - 1),
                (row + half).min(grid.rows() - 1),
            ],
        }
    }

    pub(crate) const fn contains(&self, column: usize, row: usize) -> bool {
        self.low[0] <= column && column <= self.high[0] && self.low[1] <= row && row <= self.high[1]
    }

    const fn cells(&self) -> usize {
        (self.high[0] - self.low[0] + 1) * (self.high[1] - self.low[1] + 1)
    }

    /// The number of a cell of the window, row by row from its first.
    const fn local(&self, column: usize, row: usize) -> usize {
        (row - self.low[1]) * (self.high[0] - self.low[0] + 1) + column - self.low[0]
    }
}

impl Walkable<'_> {
    /// Whether a body blocks a walker along `segment`: a static one, or a short route's blocker.
    pub(crate) fn blocks(&self, segment: Segment) -> bool {
        let walker = self.clearance.walker();
        let radius = walker.radius;
        self.statics.blocks(segment, walker)
            || self.short.is_some_and(|short| {
                let reach = |body: &IndexedBody| radius + body.radius;
                short
                    .blockers
                    .iter()
                    .any(|body| segment.comes_within(body.at, reach(body)))
            })
    }
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
            overlay: Vec::new(),
            candidates: Vec::new(),
        }
    }

    /// Plans the route from `start` to `goal`, both taken to the nearest point of the bounds, on
    /// `walkable`, into `waypoints` at the goal's height. The route ends on the goal when the walker
    /// may stand there, even in a cell whose center it may not, and a route reaches it. Otherwise
    /// it ends on the center of the open cell nearest the goal, or of the cell nearest it the
    /// walker reaches, ties to the lower number, or on a short route to the cheaper, then to the
    /// lower number; with no open cell, it is empty. A start in a
    /// blocked cell may leave it for an open one. A short route's goal is in its window.
    pub(crate) fn plan(
        &mut self,
        walkable: Walkable<'_>,
        start: Position,
        goal: Position,
        waypoints: &mut Vec<Position>,
    ) -> Planned {
        waypoints.clear();
        let grid = walkable.clearance.grid();
        let goal = grid.clamp(goal);
        self.mark_blockers(walkable);
        let clear = !walkable.blocks(Segment::new(goal, goal));
        let from = grid.nearest_cell(start);
        // A short route stays in its window, where the regions, made over the whole grid, do not
        // tell what it reaches; a start with no open cell beside it reaches nothing they hold.
        let regions = walkable.clearance.regions();
        let leaves = regions.reach(from);
        let long = walkable.short.is_none() && !leaves.is_empty();
        let target = if clear {
            Some(grid.nearest_cell(goal))
        } else if long {
            regions.nearest(grid, leaves, goal, &mut self.candidates)
        } else {
            self.nearest_open(walkable, goal)
        };
        let Some(mut target) = target else {
            return Planned {
                cost: 0,
                expanded: 0,
                reached: false,
            };
        };
        let mut replaced = false;
        if long && !leaves.meets(regions.reach(target)) {
            target = regions
                .nearest(grid, leaves, goal, &mut self.candidates)
                .expect("a start with a reachable set has a cell in it");
            replaced = true;
        }
        self.routes += 1;
        let search = Search {
            to: target,
            seen: 2 * self.routes,
            done: 2 * self.routes + 1,
        };
        self.open.clear();
        self.marks[from] = search.seen;
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
            if self.marks[at] == search.done {
                continue;
            }
            self.marks[at] = search.done;
            expanded += 1;
            let key = |cell: usize| (grid.center_distance(cell, goal), self.costs[cell], cell);
            if key(at) < key(nearest) {
                nearest = at;
            }
            if at == target {
                found = true;
                break;
            }
            self.expand(walkable, at, search);
        }
        let end = if found { target } else { nearest };
        self.cells.clear();
        let mut at = end;
        self.cells.push(RoutePlanner::number(at));
        while at != from {
            at = self.came_from[at] as usize;
            self.cells.push(RoutePlanner::number(at));
        }
        let reached = found && clear && !replaced;
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
            while next > 0 && !walkable.blocks(Segment::new(from_point, point(next - 1))) {
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

    /// Marks the window cells a short route's blockers block for the clearance's walker:
    /// those whose centers come closer to one than the two radii together.
    fn mark_blockers(&mut self, walkable: Walkable<'_>) {
        let Some(short) = walkable.short else {
            return;
        };
        let grid = walkable.clearance.grid();
        let radius = walkable.clearance.walker().radius;
        self.overlay.clear();
        self.overlay.resize(short.window.cells().div_ceil(64), 0);
        for body in short.blockers {
            grid.spans_closer(body.at, radius + body.radius, |cells| {
                for cell in cells {
                    let (column, row) = (cell % grid.columns(), cell / grid.columns());
                    if short.window.contains(column, row) {
                        let local = short.window.local(column, row);
                        self.overlay[local / 64] |= 1 << (local % 64);
                    }
                }
            });
        }
    }

    /// Whether a walker on `walkable` may stand in the cell at `column` and `row`: one of the grid,
    /// and of a short route's window, that neither a static body nor a blocker blocks.
    fn passable(overlay: &[u64], walkable: Walkable<'_>, column: usize, row: usize) -> bool {
        let grid = walkable.clearance.grid();
        if column >= grid.columns() || row >= grid.rows() {
            return false;
        }
        if let Some(short) = walkable.short {
            if !short.window.contains(column, row) {
                return false;
            }
            let local = short.window.local(column, row);
            if overlay[local / 64] & 1 << (local % 64) != 0 {
                return false;
            }
        }
        walkable.clearance.open(row * grid.columns() + column)
    }

    /// The cell a walker on `walkable` may stand in whose center is nearest `goal`, ties to the
    /// lower number; `None` with none. Rings of cells around the goal's go outward until a ring's
    /// centers must lie farther than the nearest found: a center `k` rings out is at least
    /// `k − ½` cells off.
    fn nearest_open(&self, walkable: Walkable<'_>, goal: Position) -> Option<usize> {
        let grid = walkable.clearance.grid();
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
                    if !RoutePlanner::passable(&self.overlay, walkable, x, z) {
                        continue;
                    }
                    let at = z * columns + x;
                    let candidate = (grid.center_distance(at, goal), at);
                    if best.is_none_or(|best| candidate < best) {
                        best = Some(candidate);
                    }
                }
            }
        }
        best.map(|(_, at)| at)
    }

    /// Reaches the cells around `at` a walker may stand in, and the search's target whether it
    /// may or not, more cheaply than before, in a fixed order.
    fn expand(&mut self, walkable: Walkable<'_>, at: usize, search: Search) {
        let grid = walkable.clearance.grid();
        let columns = grid.columns();
        let (column, row) = (at % columns, at / columns);
        let open = |column: usize, row: usize| {
            RoutePlanner::passable(&self.overlay, walkable, column, row)
        };
        let mut reached = [None; 8];
        for (slot, (dx, dz)) in [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ]
        .into_iter()
        .enumerate()
        {
            let (x, z) = (column.wrapping_add_signed(dx), row.wrapping_add_signed(dz));
            let entered = x < columns && z < grid.rows() && z * columns + x == search.to;
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
            reached[slot] = Some((z * columns + x, step));
        }
        for (next, step) in reached.into_iter().flatten() {
            let cost = self.costs[at] + step;
            let mark = self.marks[next];
            if mark == search.done || (mark == search.seen && self.costs[next] <= cost) {
                continue;
            }
            self.marks[next] = search.seen;
            self.costs[next] = cost;
            self.came_from[next] = RoutePlanner::number(at);
            let estimate = RoutePlanner::estimate(grid, next, search.to);
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
    use crate::navigation::walker::Walker;
    use crate::units::layer::Layer;
    use crate::values::bounds::Bounds;

    /// The point `(x, z)` in quarters of a meter.
    fn at(x: i64, z: i64) -> Position {
        let quarter = |value: i64| Num::from_bits(value << (Num::FRAC_BITS - 2));
        Position::new(Vec3::new(quarter(x), Num::ZERO, quarter(z))).unwrap()
    }

    /// A walker of 0.25 m on the first layer.
    fn walker() -> Walker {
        Walker {
            layer: Layer::FIRST,
            radius: Num::from_bits(1 << (Num::FRAC_BITS - 2)),
        }
    }

    /// 1 m cells from the origin, rows from z = 0, with a post of 0.25 m on the center of each
    /// `#`, for walkers of 0.25 m: a post blocks its own cell, whose center is on it, and not its
    /// neighbors', 1 m off; the walker's center keeps 0.5 m from it.
    fn walled(rows: &[&str]) -> (PathingGrid, BodyIndex) {
        let quarter = Num::from_bits(1 << (Num::FRAC_BITS - 2));
        let size = |count: usize| Num::from_int(i64::try_from(count).unwrap()).unwrap();
        let bounds = Bounds::new([Num::ZERO; 2], [size(rows[0].len()), size(rows.len())]);
        let mut grid = PathingGrid::new(
            Grid::new(Num::ONE, bounds.unwrap()).unwrap(),
            vec![walker()],
        );
        let mut ids = IdAllocator::default();
        let mut posts = Vec::new();
        for (row, line) in rows.iter().enumerate() {
            for (column, mark) in line.chars().enumerate() {
                if mark == '#' {
                    let center = |index: usize| i64::try_from(4 * index + 2).unwrap();
                    posts.push(IndexedBody {
                        id: ids.allocate(),
                        at: at(center(column), center(row)),
                        radius: quarter,
                        layer: Layer::FIRST,
                    });
                }
            }
        }
        let mut statics = BodyIndex::new(quarter);
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
        let clearance = grid.clearance(walker());
        let walkable = Walkable {
            clearance,
            statics: &statics,
            short: None,
        };
        let mut planner = RoutePlanner::new(clearance.grid());
        let mut waypoints = Vec::new();
        let (start, goal) = (at(2, 10), at(29, 11));
        let first = planner.plan(walkable, start, goal, &mut waypoints);
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
        planner.plan(walkable, at(29, 1), at(1, 21), &mut other);
        let mut again = Vec::new();
        assert_eq!(planner.plan(walkable, start, goal, &mut again), first);
        assert_eq!(again, waypoints);
        let mut fresh = Vec::new();
        assert_eq!(
            RoutePlanner::new(clearance.grid()).plan(walkable, start, goal, &mut fresh),
            first
        );
        assert_eq!(fresh, waypoints);

        // A goal on the post at (3.5, 1.5): the nearest open centers, (2.5, 1.5) and (4.5, 1.5),
        // are a meter off, and (2, 1) has the lower number. The search expands (0, 2), then (1, 1)
        // of total 14 + 10, before (1, 2) of 10 + 14, then (2, 1) of 24 + 0.
        let blocked = planner.plan(walkable, start, at(14, 6), &mut waypoints);
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
        let corner = planner.plan(walkable, start, at(14, 13), &mut waypoints);
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
        let tucked = planner.plan(walkable, start, edge, &mut waypoints);
        assert!(tucked.reached);
        assert_eq!(waypoints.last(), Some(&edge));

        // Behind a wall with no gap, down column 4: the regions tell before the search that
        // (6.5, 1.5) is out of reach, and the route goes to the nearest cell it reaches, (3, 1), a
        // meter from the wall. The search expands (0, 1), (1, 1) and (2, 1), each of total 30, then
        // (3, 1): 4 cells, where a search with no regions spreads over all 12 open cells left of
        // the wall to learn that it fails.
        let (closed, closed_statics) = walled(&["....#...", "....#...", "....#..."]);
        let closed_clearance = closed.clearance(walker());
        let closed_walkable = Walkable {
            clearance: closed_clearance,
            statics: &closed_statics,
            short: None,
        };
        let mut closed_planner = RoutePlanner::new(closed_clearance.grid());
        let walled_off = closed_planner.plan(closed_walkable, at(2, 6), at(26, 6), &mut waypoints);
        assert_eq!(
            walled_off,
            Planned {
                cost: 30,
                expanded: 4,
                reached: false,
            }
        );
        assert_eq!(waypoints, [at(14, 6)]);

        // A goal in the start's cell: nothing to expand past it.
        let here = planner.plan(walkable, start, at(3, 9), &mut waypoints);
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
