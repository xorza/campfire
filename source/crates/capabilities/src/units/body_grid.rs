use bevy_ecs::entity::Entity;
use campfire_math::Num;
use campfire_sim::{Position, StableId};

/// The bodies of a stage, as a sorted index of cells of the ground plane: a query of a box visits
/// the bodies of the cells it covers, grown by the widest body, so it meets every body that may
/// reach into the box, each once, and few others. A sort, not a grid over the map, as a map may
/// be wide and its bodies few, as the broadphase finds its pairs. A cell is twice the widest
/// body's radius, a meter at least. Its buffer stays between builds, so a build allocates nothing
/// once it has grown, and costs `n log n`; a query costs a search for each row of cells it
/// covers, and never more than a pass over every body.
#[derive(Debug, Default)]
pub(crate) struct BodyGrid {
    /// A cell's side, in raw units.
    cell: i64,
    widest: Num,
    /// Sorted by row, then column, then stable id.
    entries: Vec<GridBody>,
}

/// A body of the grid: its unit, where it stands, its radius and its cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GridBody {
    pub(crate) id: StableId,
    pub(crate) entity: Entity,
    pub(crate) at: Position,
    pub(crate) radius: Num,
    row: i64,
    column: i64,
}

/// A body to index: its unit, where it stands, and its radius, 0 for a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placed {
    pub(crate) id: StableId,
    pub(crate) entity: Entity,
    pub(crate) at: Position,
    pub(crate) radius: Num,
}

impl BodyGrid {
    /// Indexes `bodies` in place of what it held.
    pub(crate) fn rebuild(&mut self, bodies: impl IntoIterator<Item = Placed>) {
        self.entries.clear();
        self.entries
            .extend(bodies.into_iter().map(|placed| GridBody {
                id: placed.id,
                entity: placed.entity,
                at: placed.at,
                radius: placed.radius,
                row: 0,
                column: 0,
            }));
        self.widest = self
            .entries
            .iter()
            .map(|body| body.radius)
            .max()
            .unwrap_or(Num::ZERO);
        let side = self
            .widest
            .checked_mul_int(2)
            .expect("a body's radius is bounded");
        self.cell = side.max(Num::ONE).to_bits();
        let cell = self.cell;
        for body in &mut self.entries {
            let at = body.at.get();
            body.row = at.z.to_bits().div_euclid(cell);
            body.column = at.x.to_bits().div_euclid(cell);
        }
        self.entries
            .sort_unstable_by_key(|body| (body.row, body.column, body.id));
    }

    /// Calls `visit` with each body whose disc may reach into the box of the ground plane from
    /// `low` to `high`, `[x, z]` each, each body once, in no order a caller may rely on.
    pub(crate) fn visit(&self, low: [Num; 2], high: [Num; 2], mut visit: impl FnMut(&GridBody)) {
        if self.entries.is_empty() {
            return;
        }
        let grow = self.widest.to_bits();
        let cell = |bits: i64| bits.div_euclid(self.cell);
        let span = |axis: usize| {
            let from = cell(low[axis].to_bits().saturating_sub(grow));
            from..=cell(high[axis].to_bits().saturating_add(grow))
        };
        let (columns, rows) = (span(0), span(1));
        let inside = |body: &&GridBody| rows.contains(&body.row) && columns.contains(&body.column);
        // A box over more rows than there are bodies costs less as one pass over them all.
        let row_count = rows.end().abs_diff(*rows.start()).saturating_add(1);
        if row_count > self.entries.len() as u64 {
            self.entries.iter().filter(inside).for_each(visit);
            return;
        }
        for row in rows.clone() {
            let start = self
                .entries
                .partition_point(|body| (body.row, body.column) < (row, *columns.start()));
            let run = self.entries[start..]
                .iter()
                .take_while(|body| body.row == row && body.column <= *columns.end());
            run.for_each(&mut visit);
        }
    }

    /// Calls `visit` with each body whose disc may come within `reach` of `at` on the ground
    /// plane, as `visit` does.
    pub(crate) fn visit_near(&self, at: Position, reach: Num, visit: impl FnMut(&GridBody)) {
        let at = at.get();
        let low = [at.x, at.z].map(|axis| axis.checked_sub(reach).unwrap_or(Num::MIN));
        let high = [at.x, at.z].map(|axis| axis.checked_add(reach).unwrap_or(Num::MAX));
        self.visit(low, high, visit);
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::world::World;
    use campfire_math::Vec3;
    use campfire_sim::IdAllocator;

    use super::*;

    /// `tenths` of a meter.
    fn m(tenths: i64) -> Num {
        Num::from_int(tenths).unwrap() / 10
    }

    /// A body of `radius` tenths at `[x, z]` tenths, with the next id.
    fn placed(world: &mut World, ids: &mut IdAllocator, [x, z]: [i64; 2], radius: i64) -> Placed {
        Placed {
            id: ids.allocate(),
            entity: world.spawn_empty().id(),
            at: Position::new(Vec3::new(m(x), Num::ZERO, m(z))).unwrap(),
            radius: m(radius),
        }
    }

    /// The ids `grid` visits for the box from `low` to `high`, in tenths, sorted, each once.
    fn visited(grid: &BodyGrid, low: [i64; 2], high: [i64; 2]) -> Vec<StableId> {
        let mut ids = Vec::new();
        grid.visit(low.map(m), high.map(m), |body| ids.push(body.id));
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "each body once");
        ids
    }

    #[test]
    fn a_box_visits_the_bodies_of_the_cells_it_covers_grown_by_the_widest() {
        let (mut world, mut ids) = (World::new(), IdAllocator::default());
        let mut grid = BodyGrid::default();
        assert_eq!(visited(&grid, [0, 0], [10, 10]), []);
        // Bodies of radius 0.5 m: a cell is 2 × 0.5 = 1 m.
        let at = [[0, 0], [20, 0], [50, 0], [-30, 0], [0, 40]];
        let bodies = at.map(|at| placed(&mut world, &mut ids, at, 5));
        grid.rebuild(bodies);
        let [a, b, _, d, e] = bodies.map(|body| body.id);
        // x 1 to 3 m, grown to 0.5 to 3.5, columns 0 to 3; z -1 to 1 m, grown to -1.5 to 1.5,
        // rows -2 to 1, 4 rows of 5 bodies: the cells of a, at column 0, and b, at 2.
        assert_eq!(visited(&grid, [10, -10], [30, 10]), [a, b]);
        // z -10 to 10 m covers 22 rows, more than the 5 bodies, so one pass over them all finds
        // the same cells: e, at column 0 row 4, too.
        assert_eq!(visited(&grid, [10, -100], [30, 100]), [a, b, e]);
        // Cells below 0 round down: -3.2 to -2.8 m, grown to -3.7 to -2.3, columns -4 to -3.
        assert_eq!(visited(&grid, [-32, -2], [-28, 2]), [d]);
        // Points alone take cells of a meter: -0.4 to 0.4 m is columns and rows -1 to 0.
        grid.rebuild(at.map(|at| placed(&mut world, &mut ids, at, 0)));
        assert_eq!(visited(&grid, [-4, -4], [4, 4]).len(), 1);
    }

    #[test]
    fn a_box_meets_every_body_whose_square_overlaps_it() {
        let (mut world, mut ids) = (World::new(), IdAllocator::default());
        // A lattice 0.7 m apart from -5.6 to 5.6 m, radii 0 to 1.2 m.
        let mut bodies = Vec::new();
        for row in -8_i64..=8 {
            for column in -8..=8 {
                let radius = (row * 3 + column * 5).rem_euclid(13);
                bodies.push(placed(&mut world, &mut ids, [column * 7, row * 7], radius));
            }
        }
        let mut grid = BodyGrid::default();
        grid.rebuild(bodies.iter().copied());
        for (low, high) in [
            ([0, 0], [0, 0]),
            ([-13, 4], [9, 21]),
            ([-60, -60], [60, 60]),
            ([-200, -3], [200, 3]),
            ([33, -47], [34, -46]),
        ] {
            let got = visited(&grid, low, high);
            let overlaps = |body: &&Placed| {
                let at = body.at.get();
                let near = |axis: Num, low: i64, high: i64| {
                    m(low) <= axis + body.radius && axis - body.radius <= m(high)
                };
                near(at.x, low[0], high[0]) && near(at.z, low[1], high[1])
            };
            for body in bodies.iter().filter(overlaps) {
                assert!(
                    got.binary_search(&body.id).is_ok(),
                    "{low:?} {high:?} {body:?}"
                );
            }
        }
    }
}
