use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_declarations::Cache;
use campfire_geometry::Grid;
use campfire_math::{Num, Vec3};

use crate::body_index::{BodyIndex, IndexedBody};
use crate::crowd_costs::CrowdPrice;
use crate::pathing_grid::Clearance;
use crate::regions::Candidate;
use crate::route_planner::open_cells::OpenCells;
use crate::segment::Segment;

pub(crate) mod open_cells;

/// Plans routes by A* on a clearance of the pathing grid: eight neighbors, a straight step costing
/// 10 and a diagonal 14, a long route's step into a cell a unit stands in more by the crowd's
/// price, and no diagonal past a blocked cell, so a route never cuts a blocked corner. The estimate is the octile distance, which never overestimates those costs, so the route
/// is a cheapest one; among equal totals the cell with the lower estimate goes first, then the
/// lower cell number, so every run plans the same route. A goal a walker cannot stand on gives way
/// to the open cell nearest it, and a goal no route reaches to the nearest cell the walker reaches,
/// which the clearance's regions find before the search; a short route, which they do not serve,
/// ends on the reached cell nearest it. The route then keeps only the cells where the straight line
/// from the waypoint before would overlap a body, touch a cell the walls block or a priced one,
/// each tested exactly, so a route that goes round a crowd stays round it. The buffers stay between routes, and a route touches only the cells it reaches. Its
/// work, long routes and short, counts against one limit a tick, as many units as the grid has
/// cells: a cell expanded, a cell scanned for the nearest one, a line tested against the bodies,
/// and a cell a line's test against the walls visits.
#[derive(Resource, Debug)]
pub struct RoutePlanner {
    /// The route each cell was last reached in, as twice its number, plus one once expanded.
    marks: Vec<u64>,
    /// Each reached cell's cost from the start, and the cell it was reached from.
    costs: Vec<u32>,
    came_from: Vec<u32>,
    /// The number of searches run.
    routes: u64,
    open: OpenCells,
    /// The cells of the route, the last first.
    cells: Vec<u32>,
    /// The cells the search expanded, in order, for the nearest to the goal when it fails.
    expanded: Vec<u32>,
    /// A short route's window cells its blockers block, one bit a cell, row by row.
    overlay: Vec<u64>,
    /// The regions the nearest reachable cell to a goal may lie in.
    candidates: Vec<Candidate>,
    /// The work a tick may do, and the work done since the tick began.
    limit: u64,
    spent: u64,
}

/// Where a route may go: the clearance of its walker with the static bodies it was marked from;
/// on a short route, its window and blockers, all of the walker's layer; and on a long route, what
/// the units that stand cost it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Walkable<'a> {
    pub(crate) clearance: Clearance<'a>,
    pub(crate) statics: &'a BodyIndex,
    pub(crate) short: Option<Short<'a>>,
    pub(crate) crowd: Option<CrowdPrice<'a>>,
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

/// The way a walker passes, which a friend that steps aside clears: the walker's place, then the
/// points of its route ahead, and the walker's radius.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Passage<'a> {
    pub(crate) points: &'a [Vec3],
    pub(crate) radius: Num,
}

/// One search's target, its column and row, none for a search that floods its window, and its
/// marks.
#[derive(Debug, Clone, Copy)]
struct Search {
    to: Option<usize>,
    at: Option<[usize; 2]>,
    seen: u64,
    done: u64,
}

/// What planning a route cost, and whether it ends on the goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Planned {
    /// The cost of the route's cells, 10 a straight step, which only the tests read.
    #[cfg(test)]
    pub(crate) cost: u32,
    /// The cells the search expanded, which only the tests read.
    #[cfg(test)]
    pub(crate) expanded: u32,
    /// All the work it did, the cells expanded among it.
    pub(crate) work: u32,
    pub(crate) reached: bool,
}

impl Window {
    /// The cells up to `half` cells from `cell` along each axis, within `grid`.
    pub(crate) fn around(grid: &Grid, cell: usize, half: usize) -> Self {
        let (column, row) = (cell % grid.columns(), cell / grid.columns());
        Self {
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

impl<'a> Walkable<'a> {
    /// What a route may cross, for any route: the cells of `clearance`, past the bodies of
    /// `statics`.
    pub(crate) const fn of(clearance: Clearance<'a>, statics: &'a BodyIndex) -> Self {
        Walkable {
            clearance,
            statics,
            short: None,
            crowd: None,
        }
    }

    /// It, priced by `crowd`, as a long route is (04-capabilities/navigation.md, A route prices the
    /// units on it).
    pub(crate) const fn priced(self, crowd: Option<CrowdPrice<'a>>) -> Self {
        Self { crowd, ..self }
    }

    /// Whether something blocks a walker along `segment`: a static body, a cell the walls block it
    /// from, a short route's blocker, or a cell the crowd prices.
    pub(crate) fn blocks(&self, segment: Segment) -> bool {
        self.blocks_counting(segment, &mut 0)
    }

    /// Whether something blocks a walker along `segment`, as `blocks` says; each cell the walls'
    /// test visits adds to `work`.
    fn blocks_counting(&self, segment: Segment, work: &mut u32) -> bool {
        let walker = self.clearance.walker();
        let radius = walker.radius;
        let stood = self.crowd.map(CrowdPrice::stood);
        self.statics.blocks(segment, walker)
            || self.clearance.is_walled(segment, stood, work)
            || self.short.is_some_and(|short| {
                short
                    .blockers
                    .iter()
                    .any(|body| body.comes_closer(segment, radius))
            })
    }
}

impl Passage<'_> {
    /// Whether `body` comes no closer to any leg of the passage than the walker's radius, so the
    /// walker passes it.
    pub(crate) fn clears(&self, body: IndexedBody) -> bool {
        self.points
            .windows(2)
            .all(|leg| !body.comes_closer(Segment::new(leg[0], leg[1]), self.radius))
    }
}

impl RoutePlanner {
    const STRAIGHT: u32 = 10;
    const DIAGONAL: u32 = 14;

    /// A planner for routes on `grid`'s cells.
    pub fn new(grid: &Grid) -> Self {
        Self::of_cells(grid.cells())
    }

    fn of_cells(cells: usize) -> Self {
        Self {
            marks: vec![0; cells],
            costs: vec![0; cells],
            came_from: vec![0; cells],
            routes: 0,
            open: OpenCells::new(),
            cells: Vec::new(),
            expanded: Vec::new(),
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
    pub(crate) const fn is_spent(&self) -> bool {
        self.spent >= self.limit
    }

    /// Plans the route from `start` to `goal`, both taken to the nearest point of the bounds, on
    /// `walkable`, into `waypoints` at the goal's height. The route ends on the goal when the
    /// walker may stand there, even in a cell whose center it may not, and a route reaches it.
    /// Otherwise it ends on the center of the open cell nearest the goal, or of the cell nearest it
    /// the walker reaches, ties to the lower number, or on a short route to the cheaper, then to
    /// the lower number; with no open cell, it is empty. A start in a blocked cell may leave it for
    /// an open one beside it, and a long route's start with none beside it leaves from the open
    /// cell nearest it. A short route's goal is in its window. Its work counts against the tick's.
    pub(crate) fn plan(
        &mut self,
        walkable: Walkable<'_>,
        start: Vec3,
        goal: Vec3,
        waypoints: &mut Vec<Vec3>,
    ) -> Planned {
        let planned = self.search(walkable, start, goal, waypoints);
        self.spent += u64::from(planned.work);
        planned
    }

    fn search(
        &mut self,
        walkable: Walkable<'_>,
        start: Vec3,
        goal: Vec3,
        waypoints: &mut Vec<Vec3>,
    ) -> Planned {
        waypoints.clear();
        let grid = walkable.clearance.grid();
        let goal = grid.clamp(goal);
        self.mark_blockers(walkable);
        let mut work = 1;
        let clear = !walkable.blocks_counting(Segment::new(goal, goal), &mut work);
        let mut from = grid.nearest_cell(start);
        // A goal the walker sees, by the test a route's smoothing makes of a line, is the route,
        // with no search of cells, so a walker in a gap no open cell holds walks to it
        // (walkers-get-through.md, A route in sight is a line).
        if clear && {
            work += 1;
            !walkable.blocks_counting(Segment::new(start, goal), &mut work)
        } {
            waypoints.push(goal);
            return Planned {
                #[cfg(test)]
                cost: Self::estimate(
                    Self::place(grid, from),
                    Self::place(grid, grid.nearest_cell(goal)),
                ),
                #[cfg(test)]
                expanded: 0,
                work,
                reached: true,
            };
        }
        // A short route stays in its window, where the regions, made over the whole grid, do not
        // tell what it reaches; a start with no open cell beside it reaches nothing they hold.
        let regions = walkable.clearance.regions();
        let mut leaves = regions.reach(from);
        // A long route from deep in blocked cells, as a walker pushed into a gap narrower than its
        // clearance stands, leaves from the open cell nearest it, as it does from one beside it
        // (04-capabilities/navigation.md, A route leaves a blocked start).
        if leaves.is_empty()
            && walkable.short.is_none()
            && let Some(open) = self.nearest_open(walkable, start, &mut work)
        {
            from = open;
            leaves = regions.reach(open);
        }
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
                #[cfg(test)]
                cost: 0,
                #[cfg(test)]
                expanded: 0,
                work,
                reached: false,
            };
        };
        let replaced = long && !leaves.meets(regions.reach(target));
        if replaced {
            target = regions
                .nearest(grid, leaves, goal, &mut self.candidates, &mut work)
                .expect("a start with a reachable set has a cell in it");
        }
        self.routes += 1;
        let search = Search {
            to: Some(target),
            at: Some(Self::place(grid, target)),
            seen: 2 * self.routes,
            done: 2 * self.routes + 1,
        };
        self.open.clear();
        self.marks[from] = search.seen;
        self.costs[from] = 0;
        self.came_from[from] = Self::number(from);
        let estimate = Self::estimate(Self::place(grid, from), Self::place(grid, target));
        self.open.push(estimate, estimate, Self::number(from));
        self.expanded.clear();
        let mut found = false;
        while let Some(cell) = self.open.pop() {
            let at = cell as usize;
            if self.marks[at] == search.done {
                continue;
            }
            self.marks[at] = search.done;
            self.expanded.push(cell);
            if at == target {
                found = true;
                break;
            }
            self.expand(walkable, at, search);
        }
        // A search that fails ends on the expanded cell nearest the goal, by its center's
        // distance, then its cost, then its number: a cell's cost is final once it is expanded.
        let end = if found {
            target
        } else {
            let key = |cell: u32| {
                let at = cell as usize;
                (grid.center_distance(at, goal), self.costs[at], cell)
            };
            let nearest = self.expanded.iter().copied().min_by_key(|&cell| key(cell));
            nearest.map_or(from, |cell| cell as usize)
        };
        self.cells.clear();
        let mut at = end;
        self.cells.push(Self::number(at));
        while at != from {
            at = self.came_from[at] as usize;
            self.cells.push(Self::number(at));
        }
        let reached = found && clear && !replaced;
        let last = if reached {
            goal
        } else {
            grid.center(end, goal.y)
        };
        work += self.smooth(walkable, start, last, waypoints);
        let expanded = Self::number(self.expanded.len());
        Planned {
            #[cfg(test)]
            cost: self.costs[end],
            #[cfg(test)]
            expanded,
            work: work + expanded,
            reached,
        }
    }

    /// Whether a walker on `walkable` goes along `segment` with nothing in its way, as a route's
    /// smoothing tests a line; the test's work counts against the tick's.
    pub(crate) fn sees(&mut self, walkable: Walkable<'_>, segment: Segment) -> bool {
        let mut work = 1;
        let blocked = walkable.blocks_counting(segment, &mut work);
        self.spent += u64::from(work);
        !blocked
    }

    /// Writes into `waypoints` the route's cells from `start` to `last`, at its height, keeping
    /// only those where the straight line on from the waypoint before would be blocked; its work,
    /// a line tested and each cell the walls' test visited.
    fn smooth(
        &self,
        walkable: Walkable<'_>,
        start: Vec3,
        last: Vec3,
        waypoints: &mut Vec<Vec3>,
    ) -> u32 {
        let grid = walkable.clearance.grid();
        let y = last.y;
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
                !walkable.blocks_counting(Segment::new(from, point(next - 1)), &mut tests)
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
            body.spans_closer(grid, radius, |cells| {
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
    /// and of a short route's window, that neither a static body, a blocker nor a unit hemmed in
    /// its price walls blocks.
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
            if overlay[local / 64] & (1 << (local % 64)) != 0 {
                return false;
            }
        }
        let cell = row * grid.columns() + column;
        walkable.clearance.open(cell) && !walkable.crowd.is_some_and(|crowd| crowd.walls(cell))
    }

    /// The cell a walker on `walkable` may stand in whose center is nearest `goal`, ties to the
    /// lower number; `None` with none. Rings of cells around the goal's go outward, each by its
    /// edge alone, until a ring's centers must lie farther than the nearest found, as a center
    /// `k` rings out is at least `k − ½` cells off, or the rings leave the cells a walker may
    /// stand in: the grid, or a short route's window. So a search visits each cell of those once
    /// at most, each counted in `work`.
    pub(crate) fn nearest_open(
        &self,
        walkable: Walkable<'_>,
        goal: Vec3,
        work: &mut u32,
    ) -> Option<usize> {
        let grid = walkable.clearance.grid();
        let columns = grid.columns();
        let (low, high) = walkable.short.map_or_else(
            || ([0, 0], [columns - 1, grid.rows() - 1]),
            |short| (short.window.low, short.window.high),
        );
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
            if !Self::passable(&self.overlay, walkable, x, z) {
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

    /// Where a walker of `walkable` stands for `position`: `position` itself when nothing blocks it
    /// there, else the center of the nearest cell it may stand in, at `position`'s height, as a
    /// route's goal gives way; `None` on a grid with no such cell.
    pub(crate) fn stand_at(&self, walkable: Walkable<'_>, position: Vec3) -> Option<Vec3> {
        if !walkable.blocks(Segment::new(position, position)) {
            return Some(position);
        }
        let cell = self.nearest_open(walkable, position, &mut 0)?;
        Some(walkable.clearance.grid().center(cell, position.y))
    }

    /// Where `body`, a unit of `walkable`, steps aside to so that it clears `passage`, as Zero
    /// Hour's `getMoveAwayFromPath` floods outward from the ally for the first free cell off the
    /// path (04-capabilities/navigation.md, A friend yields to the nearest place clear of the
    /// way): `to`, its step off the walker's way, when no static body blocks it there and it
    /// clears the passage there, as the units it steps into step aside in turn; else the center
    /// of the cell of the short route's window nearest the unit by a route's steps that it may
    /// stand in, past the short route's blockers, and clears the passage from, at its height,
    /// ties to the lower total, then estimate, then number; `None` with no such cell. Its work
    /// counts against the tick's.
    pub(crate) fn yield_place(
        &mut self,
        walkable: Walkable<'_>,
        body: IndexedBody,
        to: Vec3,
        passage: Passage<'_>,
    ) -> Option<Vec3> {
        debug_assert!(walkable.short.is_some(), "a step aside stays in its window");
        let mut work = 1;
        let at = |position| IndexedBody { position, ..body };
        let open = Walkable {
            short: None,
            ..walkable
        };
        if !open.blocks_counting(Segment::new(to, to), &mut work) && passage.clears(at(to)) {
            self.spent += u64::from(work);
            return Some(to);
        }
        self.mark_blockers(walkable);
        let grid = walkable.clearance.grid();
        let from = grid.nearest_cell(body.position);
        self.routes += 1;
        let search = Search {
            to: None,
            at: None,
            seen: 2 * self.routes,
            done: 2 * self.routes + 1,
        };
        self.open.clear();
        self.marks[from] = search.seen;
        self.costs[from] = 0;
        self.open.push(0, 0, Self::number(from));
        let mut place = None;
        while let Some(cell) = self.open.pop() {
            let cell = cell as usize;
            if self.marks[cell] == search.done {
                continue;
            }
            self.marks[cell] = search.done;
            work += 1;
            let center = grid.center(cell, body.position.y);
            let [column, row] = Self::place(grid, cell);
            if Self::passable(&self.overlay, walkable, column, row) && passage.clears(at(center)) {
                place = Some(center);
                break;
            }
            self.expand(walkable, cell, search);
        }
        self.spent += u64::from(work);
        place
    }

    /// Reaches the cells around `at` a walker may stand in, and the search's target whether it
    /// may or not, more cheaply than before, in a fixed order.
    fn expand(&mut self, walkable: Walkable<'_>, at: usize, search: Search) {
        let grid = walkable.clearance.grid();
        let columns = grid.columns();
        let (column, row) = (at % columns, at / columns);
        let open = |column: usize, row: usize| Self::passable(&self.overlay, walkable, column, row);
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
            let entered = x < columns && z < grid.rows() && Some(z * columns + x) == search.to;
            if !entered && !open(x, z) {
                continue;
            }
            let step = if dx != 0 && dz != 0 {
                if !open(x, row) || !open(column, z) {
                    continue;
                }
                Self::DIAGONAL
            } else {
                Self::STRAIGHT
            };
            reached[slot] = Some((z * columns + x, [x, z], step));
        }
        for (next, place, step) in reached.into_iter().flatten() {
            let price = walkable.crowd.map_or(0, |crowd| crowd.of(next));
            let cost = self.costs[at] + step + price;
            let mark = self.marks[next];
            if mark == search.done || (mark == search.seen && self.costs[next] <= cost) {
                continue;
            }
            self.marks[next] = search.seen;
            self.costs[next] = cost;
            self.came_from[next] = Self::number(at);
            let estimate = search.at.map_or(0, |at| Self::estimate(place, at));
            self.open
                .push(cost + estimate, estimate, Self::number(next));
        }
    }

    /// The column and row of `cell`.
    const fn place(grid: &Grid, cell: usize) -> [usize; 2] {
        [cell % grid.columns(), cell / grid.columns()]
    }

    /// The octile distance between the cells at `a` and `b`, each its column and row: diagonal
    /// steps while both axes differ, then straight ones.
    fn estimate(a: [usize; 2], b: [usize; 2]) -> u32 {
        let dx = a[0].abs_diff(b[0]);
        let dz = a[1].abs_diff(b[1]);
        let (long, short) = (dx.max(dz), dx.min(dz));
        Self::STRAIGHT * Self::number(long - short) + Self::DIAGONAL * Self::number(short)
    }

    fn number(cell: usize) -> u32 {
        u32::try_from(cell).expect("a grid has at most 2²² cells")
    }
}

/// The map's install builds it, of the map's walkers.
impl Cache for RoutePlanner {
    fn installed(_: &World) -> Option<Self> {
        None
    }

    fn clear(&mut self) {
        *self = Self::of_cells(self.marks.len());
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::route_planner::RoutePlanner;

    impl RoutePlanner {
        /// The searches run so far.
        pub const fn searches(&self) -> u64 {
            self.routes
        }
    }
}

#[cfg(test)]
mod tests;
