use bevy::ecs::resource::Resource;
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
        let [ix, iy] = [column as i64 + self.origin[0], row as i64 + self.origin[1]];
        // The original keeps a ring of samples about the inner cells for its smoothed normals.
        if ix < 1 || iy < 1 || ix > self.columns - 3 || iy > self.rows - 3 {
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
}
