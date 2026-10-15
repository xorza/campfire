use bevy::ecs::resource::Resource;
use bevy::math::{Vec2, Vec3};
use campfire_client_files::{HeightGrid, TerrainQuads};

use crate::view::float_num::FloatNum;

/// The ground's height beneath a point of a map with a heightmap, the drawn ground's (design 17,
/// D8). On the grid's axes, in cells from its first sample, `u` along `x` and `v` along `z`, a
/// point's cell is its coordinates floored, and the cell's two triangles meet as its terrain's
/// quad draws them: on the diagonal from its corner 0 to 2, or from 1 to 3 when the quad is
/// flipped, its corners at the samples `(u, v)`, `(u + 1, v)`, `(u + 1, v + 1)` and
/// `(u, v + 1)`. A point off the grid takes the height of the grid's nearest point, so the
/// ground runs on level past its edge.
#[derive(Resource, Debug, Clone, PartialEq)]
pub(crate) struct GroundHeights {
    grid: HeightGrid,
    columns: usize,
    rows: usize,
    /// The first sample's point on the ground plane, `x` and `z`.
    origin: Vec2,
    cell: f32,
    step: f32,
    /// Each cell's split, row after row: whether its triangles meet on the diagonal from its
    /// corner 1 to 3.
    flips: Vec<bool>,
    /// The heights of the lowest and the highest sample, which bound where a ray meets the
    /// ground.
    lowest: f32,
    highest: f32,
}

impl GroundHeights {
    /// The heights of `grid`, each cell split as `quads` draw it, a terrain's quads over the
    /// grid's cells; with none, each from its first sample to the one across.
    pub(crate) fn of(grid: HeightGrid, quads: Option<&TerrainQuads>) -> GroundHeights {
        let columns = grid.columns() as usize;
        let rows = grid.rows() as usize;
        let flips = match quads {
            Some(quads) => {
                debug_assert!(quads.fits(&grid), "a terrain over the grid's cells");
                quads.ground.iter().map(|quad| quad.flip).collect()
            }
            None => vec![false; (columns - 1) * (rows - 1)],
        };
        let step = grid.step().float();
        let samples = grid.samples().iter().copied();
        let height = |sample: Option<u8>| f32::from(sample.expect("a grid has samples")) * step;
        GroundHeights {
            columns,
            rows,
            origin: Vec2::from(grid.origin().map(FloatNum::float)),
            cell: grid.cell().float(),
            step,
            flips,
            lowest: height(samples.clone().min()),
            highest: height(samples.max()),
            grid,
        }
    }

    /// The heightmap it is of.
    pub(crate) const fn grid(&self) -> &HeightGrid {
        &self.grid
    }

    /// The ground's height beneath the engine's `x` and `z`.
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a grid's columns and rows lie far below 2²⁴, and a clamped point's floor within them"
    )]
    pub(crate) fn at(&self, x: f32, z: f32) -> f32 {
        let point = (Vec2::new(x, z) - self.origin) / self.cell;
        let last = Vec2::new((self.columns - 1) as f32, (self.rows - 1) as f32);
        let point = point.clamp(Vec2::ZERO, last);
        let column = (point.x.floor() as usize).min(self.columns - 2);
        let row = (point.y.floor() as usize).min(self.rows - 2);
        let corner = Vec2::new(column as f32, row as f32);
        self.in_cell(column, row, point - corner)
    }

    /// The height in the cell of `column` and `row`, at the fractions `along` of its side, on
    /// the triangle of its split that holds them.
    fn in_cell(&self, column: usize, row: usize, along: Vec2) -> f32 {
        let [h0, h1, h2, h3] = [
            self.sample(column, row),
            self.sample(column + 1, row),
            self.sample(column + 1, row + 1),
            self.sample(column, row + 1),
        ];
        let Vec2 { x: fu, y: fv } = along;
        let height = if self.flips[row * (self.columns - 1) + column] {
            if fu + fv <= 1.0 {
                h0 + fu * (h1 - h0) + fv * (h3 - h0)
            } else {
                h2 + (1.0 - fu) * (h3 - h2) + (1.0 - fv) * (h1 - h2)
            }
        } else if fu >= fv {
            h0 + fu * (h1 - h0) + fv * (h2 - h1)
        } else {
            h0 + fv * (h3 - h0) + fu * (h2 - h3)
        };
        height * self.step
    }

    /// Where the ray from `origin` along `direction` first meets the ground, walking the cells it
    /// crosses, on the grid and past it, from where it falls below the highest sample to where it
    /// falls below the lowest: along a piece of the ray within one cell's triangle, or within one
    /// cell past the grid, its height over the ground is linear, so the first piece whose far end
    /// lies below the ground holds the crossing. `None` for a ray that does not fall.
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "cells within the world's bound, an inner cell's of no sign, and a grid's columns and rows far below 2²⁴"
    )]
    pub(crate) fn hit(&self, origin: Vec3, direction: Vec3) -> Option<Vec3> {
        if direction.y >= 0.0 {
            return None;
        }
        let fall = |height: f32| (height - origin.y) / direction.y;
        let start = fall(self.highest).max(0.0);
        let end = fall(self.lowest).max(start);
        // The ray on the grid's axes, in cells.
        let grid = |t: f32| {
            let at = origin + direction * t;
            (Vec2::new(at.x, at.z) - self.origin) / self.cell
        };
        let pace = Vec2::new(direction.x, direction.z) / self.cell;
        let above = |t: f32| {
            let at = origin + direction * t;
            at.y - self.at(at.x, at.z)
        };
        let first = grid(start).floor();
        let mut cell = [first.x as i64, first.y as i64];
        let inner = |[column, row]: [i64; 2]| {
            (0..self.columns as i64 - 1).contains(&column)
                && (0..self.rows as i64 - 1).contains(&row)
        };
        let mut from = start;
        loop {
            // Where the ray leaves the cell: the nearest of its next edges on each axis.
            let leave = |axis: usize| {
                if pace[axis] == 0.0 {
                    return f32::INFINITY;
                }
                let edge = if pace[axis] > 0.0 {
                    cell[axis] + 1
                } else {
                    cell[axis]
                } as f32;
                (edge - grid(from)[axis]) / pace[axis] + from
            };
            let [across, along] = [leave(0), leave(1)];
            // A float off at a cell's edge never steps the walk back.
            let to = across.min(along).min(end).max(from);
            if above(from) <= 0.0 {
                return Some(origin + direction * from);
            }
            // A cell of the grid splits where the ray crosses its diagonal.
            let mut diagonal = None;
            if inner(cell) {
                let corner = Vec2::new(cell[0] as f32, cell[1] as f32);
                let flip = self.flips[cell[1] as usize * (self.columns - 1) + cell[0] as usize];
                let side = |t: f32| {
                    let Vec2 { x: fu, y: fv } = grid(t) - corner;
                    if flip { fu + fv - 1.0 } else { fv - fu }
                };
                let (before, after) = (side(from), side(to));
                if (before > 0.0) != (after > 0.0) {
                    diagonal = Some(from + (to - from) * before / (before - after));
                }
            }
            let mut start = from;
            for piece in diagonal.into_iter().chain([to]) {
                let (high, low) = (above(start), above(piece));
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
            if across <= along {
                cell[0] += if pace[0] > 0.0 { 1 } else { -1 };
            } else {
                cell[1] += if pace[1] > 0.0 { 1 } else { -1 };
            }
        }
    }

    fn sample(&self, column: usize, row: usize) -> f32 {
        f32::from(self.grid.samples()[row * self.columns + column])
    }
}

#[cfg(test)]
mod tests {
    use campfire_client_files::GroundQuad;
    use campfire_math::Num;

    use super::*;

    /// 3 × 3 samples 10 m apart, a step of 0.625 m, the first at `x = −10`, `z = −20`: the sample
    /// `[u, v]` lies at `x = −10 + 10u`, `z = −20 + 10v`. Its heights, in steps and in meters:
    ///
    /// ```text
    /// v = 0:  0 16 32    0 10 20
    /// v = 1:  0 16 32    0 10 20
    /// v = 2:  0  0  0    0  0  0
    /// ```
    fn grid() -> HeightGrid {
        HeightGrid::new(
            [Num::int(-10), Num::int(-20)],
            Num::int(10),
            Num::from_bits(5 << (Num::FRAC_BITS - 3)),
            3,
            vec![0, 16, 32, 0, 16, 32, 0, 0, 0],
        )
        .unwrap()
    }

    /// The grid's four cells, those of `flipped` flipped.
    fn quads(flipped: [bool; 4]) -> TerrainQuads {
        TerrainQuads {
            columns: 3,
            ground: flipped
                .into_iter()
                .map(|flip| GroundQuad {
                    uv: [Vec2::ZERO; 4],
                    flip,
                })
                .collect(),
            overlays: Vec::new(),
        }
    }

    #[test]
    fn a_height_lies_on_the_triangle_its_cell_draws_and_the_grids_edge_runs_on_past_it() {
        let grid = grid();
        let unflipped = GroundHeights::of(grid.clone(), Some(&quads([false; 4])));
        let flipped = GroundHeights::of(grid.clone(), Some(&quads([false, false, true, true])));
        // Cell 2, of column 0 and row 1, has corners of 0, 10, 0 and 0 m. At (−5, −7.5), its
        // fractions 0.5 and 0.25: split from corner 0 to 2, the lower triangle, 0 + 0.5 · 10 +
        // 0.25 · (0 − 10) = 2.5; split from 1 to 3, the triangle of corner 0, 0 + 0.5 · 10 +
        // 0.25 · 0 = 5.
        assert_eq!(unflipped.at(-5.0, -7.5), 2.5);
        assert_eq!(flipped.at(-5.0, -7.5), 5.0);
        // At (−2.5, −2.5), fractions 0.75 and 0.75: on the diagonal from 0 to 2, 0 + 7.5 + 0.75
        // · (0 − 10) = 0; past the diagonal from 1 to 3, 0 + 0.25 · 0 + 0.25 · (10 − 0) = 2.5.
        assert_eq!(unflipped.at(-2.5, -2.5), 0.0);
        assert_eq!(flipped.at(-2.5, -2.5), 2.5);
        // With no terrain, each cell splits from corner 0 to 2.
        assert_eq!(GroundHeights::of(grid, None), unflipped);
        // On a sample, its height: `[1, 1]`, at (0, −10), 10 m.
        assert_eq!(flipped.at(0.0, -10.0), 10.0);
        // Past the grid, the nearest point of its edge: east of it at `v = 1.5`, halfway from
        // `[2, 1]`'s 20 m to `[2, 2]`'s 0, on either split; beyond a corner, the corner's sample.
        assert_eq!(unflipped.at(100.0, -5.0), 10.0);
        assert_eq!(flipped.at(100.0, -5.0), 10.0);
        assert_eq!(unflipped.at(100.0, -100.0), 20.0);
        assert_eq!(unflipped.at(-100.0, 100.0), 0.0);
    }

    #[test]
    fn a_ray_meets_the_ground_where_it_first_falls_below_it() {
        let grid = grid();
        let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
        for flips in [[false; 4], [false, true, true, false]] {
            let heights = GroundHeights::of(grid.clone(), Some(&quads(flips)));
            // Straight down, the height beneath.
            let hit = heights
                .hit(Vec3::new(-5.0, 100.0, -7.5), Vec3::NEG_Y)
                .unwrap();
            assert_eq!((hit.x, hit.z), (-5.0, -7.5));
            assert!(close(hit.y, heights.at(-5.0, -7.5)), "{hit}");
            // Slanting across cells, their diagonals and the grid's edge: on the ground, and
            // every point before it above it.
            for (origin, direction) in [
                (Vec3::new(-25.0, 40.0, -25.0), Vec3::new(0.7, -1.0, 0.9)),
                (Vec3::new(30.0, 30.0, 10.0), Vec3::new(-1.0, -0.6, -0.8)),
            ] {
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
            }
            // Past the grid, its edge's height.
            let hit = heights
                .hit(Vec3::new(100.0, 50.0, -5.0), Vec3::NEG_Y)
                .unwrap();
            assert!(close(hit.y, 10.0), "{hit}");
            // A ray that rises, or runs level, never meets it.
            assert_eq!(heights.hit(Vec3::new(0.0, 50.0, 0.0), Vec3::Y), None);
            assert_eq!(heights.hit(Vec3::new(0.0, 50.0, 0.0), Vec3::X), None);
        }
    }
}
