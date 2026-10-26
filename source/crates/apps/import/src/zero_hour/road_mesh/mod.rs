use bevy_math::{Vec2, Vec3};

use crate::zero_hour::format::map::map_file::MapHeights;

/// A road piece's mesh, as `loadFloat4PtSection` floats it over the ground, in `f32` as the game
/// computes it, on the game's axes, `x` east, `y` north and `z` up: its vertices' points and their
/// texture coordinates in repeats of its type's texture, `u` along it and `v` down the texture,
/// and its triangles, each counter-clockwise seen from above. One mesh is the scratch of each
/// piece in turn.
#[derive(Debug, Default)]
pub(crate) struct RoadMesh {
    pub(crate) points: Vec<Vec3>,
    pub(crate) uvs: Vec<Vec2>,
    pub(crate) triangles: Vec<[u32; 3]>,
    /// The kept columns of the piece last floated.
    columns: Vec<Column>,
}

/// A piece of road to float: the quadrilateral of ground it covers; the point its texture
/// coordinates start from, at `uv`; the road's vector along the piece, whose length sets its
/// columns, and its normal, to the left, whose length is half the width its samples across span;
/// and its scale along `u` and `v`, a quarter of its texture's repeat on each, its type's
/// `RoadWidth` but for an alpha join's `v`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RoadPiece {
    pub(crate) corners: PieceCorners,
    pub(crate) origin: Vec2,
    pub(crate) uv: Vec2,
    pub(crate) along: Vec2,
    pub(crate) normal: Vec2,
    pub(crate) scale: Vec2,
}

/// A piece's corners, `right` and `left` as seen along the road: the game's bottom left and top
/// left at its start, its bottom right and top right at its end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PieceCorners {
    pub(crate) start_right: Vec2,
    pub(crate) start_left: Vec2,
    pub(crate) end_right: Vec2,
    pub(crate) end_left: Vec2,
}

/// The ground a piece floats over, as `getMaxCellHeight` reads the map's heights.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RoadGround<'a> {
    pub(crate) heights: &'a MapHeights,
}

/// A column of a piece across the road: its right and left points, the height they both stand
/// at, and its number along the piece.
#[derive(Debug, Clone, Copy)]
struct Column {
    right: Vec2,
    left: Vec2,
    height: f32,
    at: f32,
}

/// A cell of the heightmap, 10 units wide (`MAP_XY_FACTOR`), and a step of its height
/// (`MAP_HEIGHT_SCALE`).
const CELL: f32 = 10.0;
const STEP: f32 = CELL / 16.0;
/// How far a piece floats over the highest corner under a column (`FLOAT_AMOUNT`).
const FLOAT: f32 = STEP / 8.0;
/// How far the line between a column's neighbours may pass above it for it to be dropped
/// (`MAX_ERROR`).
const MOST_ERROR: f32 = STEP * 1.1;
/// The most samples a column takes across a piece (`maxRows`).
const MOST_ACROSS: usize = 100;

impl RoadMesh {
    /// Clears the mesh, then floats `piece` over `ground` into it: a column for each cell along
    /// its vector, at least two, each sampling the ground at a point for each cell across twice its
    /// normal, from two to `MOST_ACROSS`, its points bilinear over the corners; both of a column's
    /// vertices, at its right and left corners' line, stand at the highest corner of the cells
    /// its samples lie in plus `FLOAT`. A column but the first and the last is dropped when the
    /// line from the last kept column to the next stays above it within `MOST_ERROR`, each end of
    /// the line weighed by its own distance from the column, as the game weighs them. Every piece
    /// draws whole, where the game drops one of more than 500 vertices.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the counts are the game's truncations of a length over a cell"
    )]
    pub(crate) fn float(&mut self, piece: &RoadPiece, ground: RoadGround<'_>) {
        self.points.clear();
        self.uvs.clear();
        self.triangles.clear();
        self.columns.clear();
        let PieceCorners {
            start_right,
            start_left,
            end_right,
            end_left,
        } = piece.corners;
        let along_count = ((piece.along.length() / CELL) as usize + 1).max(2);
        let across_count =
            ((2.0 * piece.normal.length() / CELL) as usize + 1).clamp(2, MOST_ACROSS);
        let right_edge = end_right - start_right;
        let start_edge = start_left - start_right;
        let end_edge = end_left - end_right;
        let left_edge = end_left - start_left + (start_edge - end_edge);
        let point = |along: f32, across: f32| {
            start_right
                + right_edge * (1.0 - across) * along
                + left_edge * across * along
                + start_edge * (1.0 - along) * across
                + end_edge * along * across
        };
        let fraction = |at: usize, count: usize| at as f32 / (count - 1) as f32;
        let column = |at: usize| {
            let along = fraction(at, along_count);
            let height = (0..across_count)
                .map(|across| ground.max_corner(point(along, fraction(across, across_count))))
                .fold(0.0, f32::max);
            Column {
                right: point(along, 0.0),
                left: point(along, 1.0),
                height,
                at: at as f32,
            }
        };
        self.columns.push(column(0));
        let mut next = column(1);
        for at in 1..along_count {
            let current = next;
            if at + 1 == along_count {
                self.columns.push(current);
                break;
            }
            next = column(at + 1);
            let last = self.columns.last().expect("the first column is kept");
            let line = (last.height * (current.at - last.at)
                + next.height * (next.at - current.at))
                / (next.at - last.at);
            if !(line >= current.height && line < current.height + MOST_ERROR) {
                self.columns.push(current);
            }
        }
        let along = piece.along.normalize_or_zero();
        let normal = piece.normal.normalize_or_zero();
        let repeat = piece.scale * 4.0;
        for column in &self.columns {
            let height = column.height + FLOAT;
            for point in [column.right, column.left] {
                let offset = point - piece.origin;
                self.points.push(point.extend(height));
                self.uvs.push(Vec2::new(
                    piece.uv.x + along.dot(offset) / repeat.x,
                    piece.uv.y - normal.dot(offset) / repeat.y,
                ));
            }
        }
        let kept = u32::try_from(self.columns.len()).expect("a piece's columns fit u32");
        for at in 1..kept {
            let [right, left] = [2 * at - 2, 2 * at - 1];
            let [next_right, next_left] = [2 * at, 2 * at + 1];
            self.triangles.push([left, right, next_right]);
            self.triangles.push([left, next_right, next_left]);
        }
    }
}

impl RoadGround<'_> {
    /// The highest corner, in world units, of the cell of the game point `point`: its point over
    /// a cell truncated, past the border, clamped to the map's cells.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the game truncates a point over a cell to its cell"
    )]
    fn max_corner(self, point: Vec2) -> f32 {
        let MapHeights {
            width,
            depth,
            border,
            samples,
            ..
        } = self.heights;
        let cell = |at: f32, side: usize| {
            let border = i64::try_from(*border).expect("a border fits i64");
            let last = i64::try_from(side).expect("a side fits i64") - 2;
            let cell = ((at / CELL) as i64 + border).clamp(0, last);
            usize::try_from(cell).expect("a cell clamped to the map's")
        };
        let [x, y] = [cell(point.x, *width), cell(point.y, *depth)];
        let sample = |x: usize, y: usize| f32::from(samples[y * width + x]);
        let high = sample(x, y)
            .max(sample(x + 1, y))
            .max(sample(x + 1, y + 1))
            .max(sample(x, y + 1));
        high * STEP
    }
}

#[cfg(test)]
mod tests;
