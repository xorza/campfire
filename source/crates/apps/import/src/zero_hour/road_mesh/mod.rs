use bevy_math::{Vec2, Vec3};
use campfire_client_files::TerrainQuads;

use crate::zero_hour::convex_clip::ConvexClip;
use crate::zero_hour::format::map::map_file::MapHeights;

/// A road piece's mesh as it lies on the ground (09-zero-hour.md, A decal lies on the ground), in `f32`
/// as the game computes it, on the game's axes, `x` east, `y` north and `z` up: its vertices'
/// points and their texture coordinates in repeats of its type's texture, `u` along it and `v`
/// down the texture, and its triangles, wound as the game winds the piece's. One mesh is the
/// scratch of each piece in turn.
#[derive(Debug, Default)]
pub(crate) struct RoadMesh {
    pub(crate) points: Vec<Vec3>,
    pub(crate) uvs: Vec<Vec2>,
    pub(crate) triangles: Vec<[u32; 3]>,
    /// A triangle of the piece clipped to the cell in turn, and to a triangle of the cell.
    in_cell: ConvexClip<Vec2>,
    part: ConvexClip<Vec2>,
}

/// A piece of road: the quadrilateral of ground it covers; the point its texture coordinates
/// start from, at `uv`; the road's direction along the piece and its normal, to the left, which
/// orient its texture; and its scale along `u` and `v`, a quarter of its texture's repeat on
/// each, its type's `RoadWidth` but for an alpha join's `v`.
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

/// The ground a piece lies on: the map's heights, and the terrain's quads over its cells, whose
/// diagonals split each cell as the client draws it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RoadGround<'a> {
    pub(crate) heights: &'a MapHeights,
    pub(crate) quads: &'a TerrainQuads,
}

/// The triangle of a cell a part lies on, by the cell's diagonal: `Low` holds the cell's south
/// corner off the diagonal, `High` its north one.
#[derive(Debug, Clone, Copy)]
enum Half {
    Low,
    High,
}

/// A line a polygon is clipped to: the side it keeps is where a point's dot with `normal` is at
/// least `at`.
#[derive(Debug, Clone, Copy)]
struct Line {
    normal: Vec2,
    at: f32,
}

/// A cell of the heightmap, 10 units wide (`MAP_XY_FACTOR`), and a step of its height
/// (`MAP_HEIGHT_SCALE`).
const CELL: f32 = 10.0;
const STEP: f32 = CELL / 16.0;
/// How far a piece floats over the ground (`FLOAT_AMOUNT`).
const FLOAT: f32 = STEP / 8.0;

impl RoadMesh {
    /// Clears the mesh, then lays `piece` on `ground` into it: the piece's two triangles, as the
    /// game splits a piece over level ground, each clipped to every cell of the height grid it
    /// covers and to each triangle of the cell's split, a part of no area dropped, as a fan of its
    /// corners, each `FLOAT` over the ground. A part past the grid's edge lies on the edge's
    /// heights, as the client runs the ground on level past it.
    pub(crate) fn lay(&mut self, piece: &RoadPiece, ground: RoadGround<'_>) {
        self.points.clear();
        self.uvs.clear();
        self.triangles.clear();
        let PieceCorners {
            start_right,
            start_left,
            end_right,
            end_left,
        } = piece.corners;
        let along = piece.along.normalize_or_zero();
        let normal = piece.normal.normalize_or_zero();
        let repeat = piece.scale * 4.0;
        let uv = |point: Vec2| {
            let offset = point - piece.origin;
            Vec2::new(
                piece.uv.x + along.dot(offset) / repeat.x,
                piece.uv.y - normal.dot(offset) / repeat.y,
            )
        };
        let cells = ground.cells();
        for triangle in [
            [start_left, start_right, end_right],
            [start_left, end_right, end_left],
        ] {
            let low = triangle.into_iter().fold(Vec2::INFINITY, Vec2::min);
            let high = triangle.into_iter().fold(Vec2::NEG_INFINITY, Vec2::max);
            let [low, high] = [low, high].map(|point| ground.cell_of(point));
            for y in low[1]..=high[1] {
                for x in low[0]..=high[0] {
                    self.in_cell.start(&triangle);
                    for line in ground.sides([x, y]) {
                        line.clip(&mut self.in_cell);
                    }
                    if (0..cells[0]).contains(&x) && (0..cells[1]).contains(&y) {
                        for half in [Half::Low, Half::High] {
                            let line = ground.diagonal([x, y], half);
                            self.part.start(self.in_cell.corners());
                            line.clip(&mut self.part);
                            self.add(|point| ground.height([x, y], half, point), uv);
                        }
                    } else {
                        self.part.start(self.in_cell.corners());
                        self.add(|point| ground.height_past(point), uv);
                    }
                }
            }
        }
    }

    /// Adds the part, each corner at `height` plus `FLOAT` and of the coordinates `uv`, as a fan
    /// from its first corner; none when it has no area.
    fn add(&mut self, height: impl Fn(Vec2) -> f32, uv: impl Fn(Vec2) -> Vec2) {
        let part = &self.part;
        if !part.has_area(|point| point) {
            return;
        }
        let first = u32::try_from(self.points.len()).expect("a piece's vertices fit u32");
        for &point in part.corners() {
            self.points.push(point.extend(height(point) + FLOAT));
            self.uvs.push(uv(point));
        }
        let count = u32::try_from(part.corners().len()).expect("a part's corners fit u32");
        self.triangles
            .extend((1..count - 1).map(|at| [first, first + at, first + at + 1]));
    }
}

impl RoadGround<'_> {
    /// The cells of the height grid on each axis, one fewer than its samples.
    fn cells(self) -> [i64; 2] {
        let MapHeights { width, depth, .. } = self.heights;
        [width, depth].map(|side| i64::try_from(*side).expect("a side fits i64") - 1)
    }

    /// The cell that holds the game point `point`, on the grid's indices, past the border: below
    /// the grid, `-1`, and past it, the count of its cells, each the region beyond its edge.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a point over a cell, floored, within the map's bound"
    )]
    fn cell_of(self, point: Vec2) -> [i64; 2] {
        let border = i64::try_from(self.heights.border).expect("a border fits i64");
        let cells = self.cells();
        [0, 1].map(|axis| ((point[axis] / CELL).floor() as i64 + border).clamp(-1, cells[axis]))
    }

    /// The game coordinate of the grid's line `at`, on the grid's indices.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a map's lines lie far below 2²⁴"
    )]
    fn line(self, at: i64) -> f32 {
        let border = i64::try_from(self.heights.border).expect("a border fits i64");
        (at - border) as f32 * CELL
    }

    /// The sides of the cell `cell`, as lines whose kept sides hold it; a region past the grid's
    /// edge has none on that edge's far side.
    fn sides(self, cell: [i64; 2]) -> impl Iterator<Item = Line> {
        let cells = self.cells();
        [0, 1].into_iter().flat_map(move |axis| {
            let normal = Vec2::AXES[axis];
            let low = (cell[axis] >= 0).then(|| Line {
                normal,
                at: self.line(cell[axis]),
            });
            let high = (cell[axis] < cells[axis]).then(|| Line {
                normal: -normal,
                at: -self.line(cell[axis] + 1),
            });
            [low, high].into_iter().flatten()
        })
    }

    /// The side of the diagonal of the cell `cell` that keeps its triangle `half`.
    fn diagonal(self, cell: [i64; 2], half: Half) -> Line {
        let [x, y] = cell.map(|at| self.line(at));
        let line = if self.crossed(cell) {
            // From the south-east corner to the north-west: the south-west side holds `Low`.
            Line {
                normal: Vec2::new(-1.0, -1.0),
                at: -(x + y + CELL),
            }
        } else {
            // From the south-west corner to the north-east: the south-east side holds `Low`.
            Line {
                normal: Vec2::new(1.0, -1.0),
                at: x - y,
            }
        };
        match half {
            Half::Low => line,
            Half::High => Line {
                normal: -line.normal,
                at: -line.at,
            },
        }
    }

    /// Whether the cell `cell`'s triangles meet on its diagonal from its south-east corner to its
    /// north-west: the game's cell `[x, y]` is the terrain file's `[x, rows − 1 − y]`, whose
    /// diagonal from its corner 1 to 3 is the game's from south-west to north-east.
    fn crossed(self, [x, y]: [i64; 2]) -> bool {
        let [columns, rows] = self
            .cells()
            .map(|count| usize::try_from(count).expect("a count"));
        let [x, y] = [x, y].map(|at| usize::try_from(at).expect("an inner cell"));
        !self.quads.ground[(rows - 1 - y) * columns + x].flip
    }

    /// The ground's height, in world units, at the game point `point` of the cell `cell`, on its
    /// triangle `half`.
    fn height(self, cell: [i64; 2], half: Half, point: Vec2) -> f32 {
        let corner = Vec2::new(self.line(cell[0]), self.line(cell[1]));
        let Vec2 { x: fu, y: fv } = (point - corner) / CELL;
        let [x, y] = cell.map(|at| usize::try_from(at).expect("an inner cell"));
        let MapHeights { width, samples, .. } = self.heights;
        let sample = |x: usize, y: usize| f32::from(samples[y * width + x]);
        let [h0, h1, h2, h3] = [
            sample(x, y),
            sample(x + 1, y),
            sample(x + 1, y + 1),
            sample(x, y + 1),
        ];
        let height = match (self.crossed(cell), half) {
            (true, Half::Low) => h0 + fu * (h1 - h0) + fv * (h3 - h0),
            (true, Half::High) => h2 + (1.0 - fu) * (h3 - h2) + (1.0 - fv) * (h1 - h2),
            (false, Half::Low) => h0 + fu * (h1 - h0) + fv * (h2 - h1),
            (false, Half::High) => h0 + fv * (h3 - h0) + fu * (h2 - h3),
        };
        height * STEP
    }

    /// The ground's height, in world units, at the game point `point` past the grid's edge: its
    /// height at the grid's nearest point, as the client runs the ground on level past the edge.
    fn height_past(self, point: Vec2) -> f32 {
        let cells = self.cells();
        let low = Vec2::splat(self.line(0));
        let high = Vec2::new(self.line(cells[0]), self.line(cells[1]));
        let point = point.clamp(low, high);
        let at = self.cell_of(point);
        let cell = [0, 1].map(|axis| at[axis].min(cells[axis] - 1));
        let corner = Vec2::new(self.line(cell[0]), self.line(cell[1]));
        let Vec2 { x: fu, y: fv } = (point - corner) / CELL;
        let low_half = if self.crossed(cell) {
            fu + fv <= 1.0
        } else {
            fu >= fv
        };
        let half = if low_half { Half::Low } else { Half::High };
        self.height(cell, half, point)
    }
}

impl Line {
    /// Clips `polygon` to the kept side, each crossing on an axis's line of the line's
    /// coordinate exactly.
    fn clip(self, polygon: &mut ConvexClip<Vec2>) {
        polygon.clip(
            |point| self.normal.dot(point) - self.at,
            |from, to, share| {
                let mut point = from + (to - from) * share;
                for axis in 0..2 {
                    if self.normal[1 - axis] == 0.0 {
                        point[axis] = self.at / self.normal[axis];
                    }
                }
                point
            },
        );
    }
}

#[cfg(test)]
mod tests;
