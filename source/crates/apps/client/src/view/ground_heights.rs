use bevy::ecs::resource::Resource;
use bevy::math::Vec3;
use campfire_capabilities::HeightGrid;
use campfire_math::Num;

use crate::view::float_num::FloatNum;

/// The ground's height beneath a point of a map with a heightmap, as Zero Hour's
/// `getHeightMapHeight` gives it, in `f32` step for step: on the original's axes, `x` east and
/// `y` north, the engine's `x` and `−z`, a point's cell is its coordinates times the inverse of
/// the cell, floored, and the cell's two triangles meet on the diagonal from its first sample to
/// the one across, whichever way the terrain draws it. A point off the inner cells takes the
/// nearest sample's height.
#[derive(Resource, Debug, Clone, PartialEq)]
pub(crate) struct GroundHeights {
    /// Row after row from the original's south, each from its west: the original's order.
    samples: Vec<u8>,
    columns: i64,
    rows: i64,
    /// The original's index of the sample at `x = 0` and `y = 0`, its border.
    origin: [i64; 2],
    /// The inverse of the cell, in `f32`, as the original multiplies by `1 / MAP_XY_FACTOR`.
    inverse: f32,
    step: f32,
}

impl GroundHeights {
    /// The heights of `grid`; `None` unless its origin lies a whole number of cells from 0 on
    /// both axes, as the original's border does.
    pub(crate) fn of(grid: &HeightGrid) -> Option<GroundHeights> {
        let columns = i64::from(grid.columns());
        let rows = i64::from(grid.rows());
        let cell = grid.cell().to_bits();
        let cells = |at: Num| (at.to_bits() % cell == 0).then_some(at.to_bits() / cell);
        let [x, z] = grid.origin();
        // The engine's first row lies at the least `z`, the original's last.
        let origin = [-cells(x)?, rows - 1 + cells(z)?];
        let width = usize::try_from(columns).expect("a grid's columns fit usize");
        let samples = grid
            .samples()
            .chunks_exact(width)
            .rev()
            .flatten()
            .copied()
            .collect();
        Some(GroundHeights {
            samples,
            columns,
            rows,
            origin,
            inverse: 1.0 / grid.cell().float(),
            step: grid.step().float(),
        })
    }

    /// The ground's height beneath the engine's `x` and `z`.
    pub(crate) fn at(&self, x: f32, z: f32) -> f32 {
        let along = |coordinate: f32| {
            let scaled = coordinate * self.inverse;
            let floor = scaled.floor();
            (floor, scaled - floor)
        };
        let (column, fx) = along(x);
        let (row, fy) = along(-z);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a floor within the world's bound"
        )]
        let cell = [column as i64 + self.origin[0], row as i64 + self.origin[1]];
        self.in_cell(cell, fx, fy)
    }

    /// The ground's height in the original's cell `[ix, iy]`, at the fractions `fx` east and `fy`
    /// north across it.
    fn in_cell(&self, [ix, iy]: [i64; 2], fx: f32, fy: f32) -> f32 {
        // The original keeps a ring of samples about the inner cells for its smoothed normals.
        if !self.inner([ix, iy]) {
            let clamp = |index: i64, extent: i64| index.clamp(0, extent - 1);
            return self.sample(clamp(ix, self.columns), clamp(iy, self.rows)) * self.step;
        }
        let p0 = self.sample(ix, iy);
        let p2 = self.sample(ix + 1, iy + 1);
        let height = if fy > fx {
            let p3 = self.sample(ix, iy + 1);
            p3 + (1.0 - fy) * (p0 - p3) + fx * (p2 - p3)
        } else {
            let p1 = self.sample(ix + 1, iy);
            p1 + fy * (p2 - p1) + (1.0 - fx) * (p0 - p1)
        };
        height * self.step
    }

    /// Whether the cell `[ix, iy]` is one of the inner cells, whose triangles the ground is.
    const fn inner(&self, [ix, iy]: [i64; 2]) -> bool {
        ix >= 1 && iy >= 1 && ix <= self.columns - 3 && iy <= self.rows - 3
    }

    /// Where the ray from `origin` along `direction` first meets the ground, walking the cells it
    /// crosses from where it falls below the highest sample to where it falls below the lowest:
    /// in a cell, its height over the ground is linear on each of the cell's planes, so the first
    /// plane whose far end lies below the ground holds the crossing; a ray that enters a cell
    /// below its ground met the step between two cells off the inner ones, where it entered.
    /// `None` for a ray that does not fall.
    pub(crate) fn hit(&self, origin: Vec3, direction: Vec3) -> Option<Vec3> {
        if direction.y >= 0.0 {
            return None;
        }
        let (low, high) = self
            .samples
            .iter()
            .fold((u8::MAX, u8::MIN), |(low, high), &sample| {
                (low.min(sample), high.max(sample))
            });
        let fall = |height: f32| (height - origin.y) / direction.y;
        let start = fall(f32::from(high) * self.step).max(0.0);
        let end = fall(f32::from(low) * self.step).max(start);
        // The ray on the original's grid: `u` east and `v` north, in cells.
        let grid = |t: f32| {
            let at = origin + direction * t;
            #[expect(clippy::cast_precision_loss, reason = "a border of few cells")]
            let [ox, oy] = self.origin.map(|cells| cells as f32);
            [at.x * self.inverse + ox, -at.z * self.inverse + oy]
        };
        let pace = [direction.x * self.inverse, -direction.z * self.inverse];
        let above = |t: f32, cell: [i64; 2]| {
            let [u, v] = grid(t);
            #[expect(
                clippy::cast_precision_loss,
                reason = "a cell within the world's bound"
            )]
            let [fx, fy] = [
                (u - cell[0] as f32).clamp(0.0, 1.0),
                (v - cell[1] as f32).clamp(0.0, 1.0),
            ];
            origin.y + direction.y * t - self.in_cell(cell, fx, fy)
        };
        let [east_at, north_at] = grid(start);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a floor within the world's bound"
        )]
        let mut cell = [east_at.floor() as i64, north_at.floor() as i64];
        let mut from = start;
        loop {
            // Where the ray leaves the cell: the nearest of its next edges on each axis.
            #[expect(
                clippy::cast_precision_loss,
                reason = "a cell within the world's bound"
            )]
            let leave = |axis: usize| {
                let edge = if pace[axis] > 0.0 {
                    cell[axis] + 1
                } else {
                    cell[axis]
                } as f32;
                if pace[axis] == 0.0 {
                    f32::INFINITY
                } else {
                    (edge - grid(from)[axis]) / pace[axis] + from
                }
            };
            let [east, north] = [leave(0), leave(1)];
            // A float off at a cell's edge never steps the walk back.
            let to = east.min(north).min(end).max(from);
            if above(from, cell) <= 0.0 {
                return Some(origin + direction * from);
            }
            // An inner cell's two planes meet on its diagonal, where `fy − fx` changes sign.
            let mut diagonal = None;
            if self.inner(cell) {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a cell within the world's bound"
                )]
                let side = |t: f32| {
                    let [u, v] = grid(t);
                    (v - cell[1] as f32) - (u - cell[0] as f32)
                };
                let (before, after) = (side(from), side(to));
                if (before > 0.0) != (after > 0.0) {
                    diagonal = Some(from + (to - from) * before / (before - after));
                }
            }
            let mut start = from;
            for piece in diagonal.into_iter().chain([to]) {
                let (high, low) = (above(start, cell), above(piece, cell));
                if low <= 0.0 {
                    let t = if high > low {
                        start + (piece - start) * high / (high - low)
                    } else {
                        start
                    };
                    return Some(origin + direction * t);
                }
                start = piece;
            }
            if to >= end {
                return None;
            }
            from = to;
            if east <= north {
                cell[0] += if pace[0] > 0.0 { 1 } else { -1 };
            } else {
                cell[1] += if pace[1] > 0.0 { 1 } else { -1 };
            }
        }
    }

    fn sample(&self, column: i64, row: i64) -> f32 {
        let at = usize::try_from(row * self.columns + column).expect("a sample of the grid");
        f32::from(self.samples[at])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_height_is_the_originals_triangle_beneath_and_the_nearest_sample_off_the_inner_cells() {
        // 5 × 5 samples 10 m apart, a border of 1: the original's sample (i, j) lies at x =
        // (i − 1) · 10 and y = (j − 1) · 10, the engine's z = −y. The engine's rows run from the
        // least z, the original's last row; each sample's height is `i + 10 · j` steps of 0.625.
        let original = |i: u8, j: u8| i + 10 * j;
        let samples: Vec<u8> = (0..5)
            .rev()
            .flat_map(|j| (0..5).map(move |i| original(i, j)))
            .collect();
        let origin = [Num::int(-10), Num::int(-30)];
        let step = Num::from_bits(5 << (Num::FRAC_BITS - 3));
        let grid = HeightGrid::new(origin, Num::int(10), step, 5, samples).unwrap();
        let heights = GroundHeights::of(&grid).unwrap();
        // The original's (12, 3): cell (1 + 1, 0 + 1), fractions 0.2 and 0.3, above the diagonal
        // as fy > fx: p3 + (1 − fy)(p0 − p3) + fx(p2 − p3) with p0 = 12, p2 = 23, p3 = 22:
        // 22 + 0.7 · (−10) + 0.2 · 1 = 15.2, times 0.625 = 9.5, as `f32` gives it.
        let expected = (22.0_f32 + (1.0 - 0.3_f32) * -10.0 + 0.2_f32 * 1.0) * 0.625;
        let fx = 12.0_f32 * 0.1 - (12.0_f32 * 0.1).floor();
        let fy = 3.0_f32 * 0.1 - (3.0_f32 * 0.1).floor();
        let p = |i: u8, j: u8| f32::from(original(i, j));
        let by_hand =
            (p(2, 2) + (1.0 - fy) * (p(2, 1) - p(2, 2)) + fx * (p(3, 2) - p(2, 2))) * 0.625;
        assert_eq!(heights.at(12.0, -3.0), by_hand);
        assert!((by_hand - expected).abs() < 1e-5 && (by_hand - 9.5).abs() < 1e-5);
        // Below the diagonal, the original's (13, 2): p1 + fy(p2 − p1) + (1 − fx)(p0 − p1), with
        // p1 = 13, p2 = 23, p0 = 12.
        let fx = 13.0_f32 * 0.1 - (13.0_f32 * 0.1).floor();
        let fy = 2.0_f32 * 0.1 - (2.0_f32 * 0.1).floor();
        let lower = (p(3, 1) + fy * (p(3, 2) - p(3, 1)) + (1.0 - fx) * (p(2, 1) - p(3, 1))) * 0.625;
        assert_eq!(heights.at(13.0, -2.0), lower);
        assert_ne!(lower, by_hand);
        // On a sample, its height: the original's (10, 10) is sample (2, 2), 22 steps.
        assert_eq!(heights.at(10.0, -10.0), 22.0 * 0.625);
        // Off the inner cells, the nearest sample: the original's (−10, 25) is cell (0, 3), past
        // the first column, so sample (0, 3), 30 steps; and (100, −100) is sample (4, 0), 4 steps.
        assert_eq!(heights.at(-10.0, -25.0), 30.0 * 0.625);
        assert_eq!(heights.at(100.0, 100.0), 4.0 * 0.625);
        // A grid whose origin is no whole number of cells has no heights here.
        let grid = HeightGrid::new(
            [Num::int(-5), Num::int(-30)],
            Num::int(10),
            Num::ONE,
            2,
            vec![0; 4],
        )
        .unwrap();
        assert_eq!(GroundHeights::of(&grid), None);
    }

    #[test]
    fn a_ray_meets_the_ground_where_it_first_falls_below_it() {
        // The grid of the test above: sample (i, j) is `i + 10 · j` steps of 0.625.
        let original = |i: u8, j: u8| i + 10 * j;
        let samples: Vec<u8> = (0..5)
            .rev()
            .flat_map(|j| (0..5).map(move |i| original(i, j)))
            .collect();
        let step = Num::from_bits(5 << (Num::FRAC_BITS - 3));
        let grid = HeightGrid::new(
            [Num::int(-10), Num::int(-30)],
            Num::int(10),
            step,
            5,
            samples,
        )
        .unwrap();
        let heights = GroundHeights::of(&grid).unwrap();
        let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
        // Straight down onto the sloped cell at the original's (12, 3): its height there.
        let hit = heights
            .hit(Vec3::new(12.0, 100.0, -3.0), Vec3::NEG_Y)
            .unwrap();
        assert_eq!((hit.x, hit.z), (12.0, -3.0));
        assert!(close(hit.y, heights.at(12.0, -3.0)), "{hit}");
        // Slanting across several cells: on the ground, and every point before it above it.
        let (origin, direction) = (Vec3::new(-5.0, 60.0, 5.0), Vec3::new(0.7, -1.0, -0.4));
        let hit = heights.hit(origin, direction).unwrap();
        assert!(close(hit.y, heights.at(hit.x, hit.z)), "{hit}");
        let reach = (hit - origin).length() / direction.length();
        for at in 0..1000 {
            #[expect(clippy::cast_precision_loss, reason = "a small count")]
            let point = origin + direction * (reach * at as f32 / 1000.0);
            assert!(
                point.y >= heights.at(point.x, point.z) - 1e-3,
                "{point} below the ground before {hit}"
            );
        }
        // Off the map, the nearest sample's height: (100, −100) of the original is sample (4, 0).
        let hit = heights
            .hit(Vec3::new(100.0, 50.0, 100.0), Vec3::NEG_Y)
            .unwrap();
        assert!(close(hit.y, 4.0 * 0.625));
        // A ray that rises, or runs level, never meets it.
        assert_eq!(heights.hit(Vec3::new(0.0, 50.0, 0.0), Vec3::Y), None);
        assert_eq!(heights.hit(Vec3::new(0.0, 50.0, 0.0), Vec3::X), None);

        // A low middle within a high rim: 5 × 5 samples, 100 steps on the edge, 0 inside. A ray
        // east at 20 m, falling 0.1 m a meter, crosses the low cells and meets the rim's step
        // where it enters it, at x = 30, 18.5 m up.
        let rim: Vec<u8> = (0..25)
            .map(|at| {
                if at % 5 == 0 || at % 5 == 4 || !(5..20).contains(&at) {
                    100
                } else {
                    0
                }
            })
            .collect();
        let grid =
            HeightGrid::new([Num::int(-10), Num::int(-30)], Num::int(10), step, 5, rim).unwrap();
        let heights = GroundHeights::of(&grid).unwrap();
        let hit = heights
            .hit(Vec3::new(15.0, 20.0, -15.0), Vec3::new(1.0, -0.1, 0.0))
            .unwrap();
        assert!(
            close(hit.x, 30.0) && close(hit.y, 18.5) && close(hit.z, -15.0),
            "{hit}"
        );
    }
}
