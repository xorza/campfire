use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use campfire_math::Tick;
use campfire_sim::{Position, StableId};

use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::pathing_grid::Clearance;
use crate::navigation::regions::Candidate;
use crate::navigation::segment::Segment;
use crate::values::grid::Grid;

/// Plans routes by A* on a clearance of the pathing grid: eight neighbors, a straight step costing
/// 10 and a diagonal 14, and no diagonal past a blocked cell, so a route never cuts a blocked
/// corner. The estimate is the octile distance, which never overestimates those costs, so the route
/// is a cheapest one; among equal totals the cell with the lower estimate goes first, then the
/// lower cell number, so every run plans the same route. A goal a walker cannot stand on gives way
/// to the open cell nearest it, and a goal no route reaches to the nearest cell the walker reaches,
/// which the clearance's regions find before the search; a short route, which they do not serve,
/// ends on the reached cell nearest it. The route then keeps only the cells where the straight line
/// from the waypoint before would overlap a body, tested exactly. The buffers stay between routes,
/// and a route touches only the cells it reaches. Its work, long routes and short, counts against
/// one limit a tick, as many units as the grid has cells: a cell expanded, a cell scanned for the
/// nearest one, a line tested against the bodies.
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
    /// The work a tick may do, and the work done since the tick began.
    limit: u64,
    spent: u64,
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

/// A walker whose route waits for the planner, in the order routes are planned: by the tick it
/// asked in, then by stable id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Waiting {
    pub(crate) tick: Tick,
    pub(crate) id: StableId,
    pub(crate) entity: Entity,
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
    /// All the work it did, the cells expanded among it.
    pub(crate) work: u32,
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
            limit: u64::try_from(cells).expect("a grid has at most 2²² cells"),
            spent: 0,
        }
    }

    /// Starts a tick's work.
    pub(crate) const fn begin_tick(&mut self) {
        self.spent = 0;
    }

    /// Whether this tick did all the work it may: the plan that met the limit finished, and the
    /// rest wait for a later tick.
    pub(crate) const fn spent(&self) -> bool {
        self.spent >= self.limit
    }

    /// Plans the route from `start` to `goal`, both taken to the nearest point of the bounds, on
    /// `walkable`, into `waypoints` at the goal's height. The route ends on the goal when the
    /// walker may stand there, even in a cell whose center it may not, and a route reaches it.
    /// Otherwise it ends on the center of the open cell nearest the goal, or of the cell nearest it
    /// the walker reaches, ties to the lower number, or on a short route to the cheaper, then to
    /// the lower number; with no open cell, it is empty. A start in a blocked cell may leave it for
    /// an open one. A short route's goal is in its window. Its work counts against the tick's.
    pub(crate) fn plan(
        &mut self,
        walkable: Walkable<'_>,
        start: Position,
        goal: Position,
        waypoints: &mut Vec<Position>,
    ) -> Planned {
        let planned = self.search(walkable, start, goal, waypoints);
        self.spent += u64::from(planned.work);
        planned
    }

    fn search(
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
        let mut work = 1;
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
            regions.nearest(grid, leaves, goal, &mut self.candidates, &mut work)
        } else {
            self.nearest_open(walkable, goal, &mut work)
        };
        let Some(mut target) = target else {
            return Planned {
                cost: 0,
                expanded: 0,
                work,
                reached: false,
            };
        };
        let mut replaced = false;
        if long && !leaves.meets(regions.reach(target)) {
            target = regions
                .nearest(grid, leaves, goal, &mut self.candidates, &mut work)
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
        let last = if reached {
            goal
        } else {
            grid.center(end, goal.get().y)
        };
        work += self.smooth(walkable, start, last, waypoints);
        Planned {
            cost: self.costs[end],
            expanded,
            work: work + expanded,
            reached,
        }
    }

    /// Writes into `waypoints` the route's cells from `start` to `last`, at its height, keeping
    /// only those where the straight line on from the waypoint before would overlap a body; the
    /// lines it tested.
    fn smooth(
        &self,
        walkable: Walkable<'_>,
        start: Position,
        last: Position,
        waypoints: &mut Vec<Position>,
    ) -> u32 {
        let grid = walkable.clearance.grid();
        let y = last.get().y;
        let point = |index: usize| match index {
            0 => last,
            _ => grid.center(self.cells[index] as usize, y),
        };
        let mut tests = 0;
        let mut anchor = self.cells.len() - 1;
        let mut from = start;
        while anchor > 0 {
            let mut next = anchor - 1;
            while next > 0 && {
                tests += 1;
                !walkable.blocks(Segment::new(from, point(next - 1)))
            } {
                next -= 1;
            }
            from = point(next);
            waypoints.push(from);
            anchor = next;
        }
        if waypoints.is_empty() {
            waypoints.push(last);
        }
        tests
    }

    /// Marks the window cells a short route's blockers block for the clearance's walker:
    /// those whose centers come closer to one than the two radii together. Each row's run of a
    /// blocker's cells is cut to the window before it is marked.
    fn mark_blockers(&mut self, walkable: Walkable<'_>) {
        let Some(short) = walkable.short else {
            return;
        };
        let grid = walkable.clearance.grid();
        let radius = walkable.clearance.walker().radius;
        self.overlay.clear();
        self.overlay.resize(short.window.cells().div_ceil(64), 0);
        let window = short.window;
        let columns = grid.columns();
        for body in short.blockers {
            grid.spans_closer(body.at, radius + body.radius, |cells| {
                let row = cells.start / columns;
                if row < window.low[1] || window.high[1] < row {
                    return;
                }
                let first = (cells.start % columns).max(window.low[0]);
                let end = ((cells.end - 1) % columns).min(window.high[0]);
                for column in first..=end {
                    let local = window.local(column, row);
                    self.overlay[local / 64] |= 1 << (local % 64);
                }
            });
        }
    }

    /// Whether a walker on `walkable` may stand in the cell at `column` and `row`: one of the grid,
    /// and of a short route's window, that neither a static body nor a blocker blocks.
    const fn passable(overlay: &[u64], walkable: Walkable<'_>, column: usize, row: usize) -> bool {
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
    /// lower number; `None` with none. Rings of cells around the goal's go outward, each by its
    /// edge alone, until a ring's centers must lie farther than the nearest found, as a center
    /// `k` rings out is at least `k − ½` cells off, or the rings leave the cells a walker may
    /// stand in: the grid, or a short route's window. So a search visits each cell of those once
    /// at most, each counted in `work`.
    fn nearest_open(
        &self,
        walkable: Walkable<'_>,
        goal: Position,
        work: &mut u32,
    ) -> Option<usize> {
        let grid = walkable.clearance.grid();
        let columns = grid.columns();
        let (low, high) = walkable
            .short
            .map_or(([0, 0], [columns - 1, grid.rows() - 1]), |short| {
                (short.window.low, short.window.high)
            });
        let middle = grid.nearest_cell(goal);
        let (column, row) = (middle % columns, middle / columns);
        let rings = [
            column.abs_diff(low[0]),
            column.abs_diff(high[0]),
            row.abs_diff(low[1]),
            row.abs_diff(high[1]),
        ];
        let rings = rings.into_iter().max().expect("four distances");
        let cell = u128::from(grid.cell().to_bits().unsigned_abs());
        let mut best: Option<(u128, usize)> = None;
        let mut visit = |best: &mut Option<(u128, usize)>, x: usize, z: usize| {
            *work += 1;
            if !RoutePlanner::passable(&self.overlay, walkable, x, z) {
                return;
            }
            let at = z * columns + x;
            let candidate = (grid.center_distance(at, goal), at);
            if best.is_none_or(|best| candidate < best) {
                *best = Some(candidate);
            }
        };
        for ring in 0..=rings {
            if let Some((distance, _)) = best
                && ring > 0
            {
                let least = (2 * ring as u128 - 1) * cell;
                if least * least > distance {
                    break;
                }
            }
            let left = column.checked_sub(ring).filter(|&x| x >= low[0]);
            let right = Some(column + ring).filter(|&x| x <= high[0] && ring > 0);
            let across = column.saturating_sub(ring).max(low[0])..=(column + ring).min(high[0]);
            for z in row.saturating_sub(ring).max(low[1])..=(row + ring).min(high[1]) {
                if z.abs_diff(row) == ring {
                    for x in across.clone() {
                        visit(&mut best, x, z);
                    }
                } else {
                    for x in left.into_iter().chain(right) {
                        visit(&mut best, x, z);
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
mod tests;
