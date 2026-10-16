use std::ops::RangeInclusive;

use campfire_common::{EnumAll, NamedEnum};
use campfire_math::{Num, Rounding, Vec3};
use campfire_sim::Position;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::GroundError;
use crate::height_grid::HeightGrid;

/// The diagonal every cell of a map's ground splits on, between its corners 0 to 3 at the
/// samples `(u, v)`, `(u + 1, v)`, `(u + 1, v + 1)` and `(u, v + 1)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, EnumAll, NamedEnum)]
pub enum CellSplit {
    /// From corner 0 to 2.
    #[default]
    #[name = "0_2"]
    ZeroTwo,
    /// From corner 1 to 3.
    #[name = "1_3"]
    OneThree,
}

/// Written and read as serde's derive does a unit variant, by its name in data.
impl Serialize for CellSplit {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.serialize_member(serializer)
    }
}

impl<'de> Deserialize<'de> for CellSplit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize_member(deserializer)
    }
}

/// The span of a ground's coarse cells: squares of `cell` meters that start at `from` and run
/// toward `to` on each axis, `[x, z]`, as many as cover the span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoarseSpan {
    pub cell: Num,
    pub from: [Num; 2],
    pub to: [Num; 2],
}

/// The ground of a map, as the sim measures it (20-zero-hour-projectiles.md, The sim's ground is
/// the map's height grid): flat at height 0, or its height grid, each cell split on the map's one
/// diagonal, with the highest sample of each coarse cell. Package data, not state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ground(Option<Heights>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Heights {
    grid: HeightGrid,
    split: CellSplit,
    coarse: CoarseCells,
}

/// The coarse cells of a span, each axis's own way, and the highest sample of each, in steps,
/// row after row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CoarseCells {
    span: CoarseSpan,
    counts: [u32; 2],
    highest: Vec<u8>,
}

impl Ground {
    /// The ground of `grid`, split on `split`, with the coarse cells of `span`, or the grid's own
    /// cells with none; an error for a span whose cell is smaller than the grid's, so a square may
    /// hold no sample, that spans nothing on an axis, that lies past the world's bound, so its
    /// arithmetic cannot overflow, or of more cells than the grid has samples, so its table is
    /// never larger than the grid it is of. A span may reach past the grid: a
    /// square there holds the samples of the grid's edge, as the original's height past its grid
    /// is its clamped sample (`getClipHeight`).
    pub fn new(
        grid: HeightGrid,
        split: CellSplit,
        span: Option<CoarseSpan>,
    ) -> Result<Self, GroundError> {
        let span = span.unwrap_or_else(|| CoarseSpan {
            cell: grid.cell(),
            from: grid.origin(),
            to: grid.far_corner(),
        });
        if span.cell < grid.cell() {
            return Err(GroundError::CoarseCell);
        }
        if (0..2).any(|axis| span.from[axis] == span.to[axis]) {
            return Err(GroundError::CoarseSpan);
        }
        let within = |[x, z]: [Num; 2]| Position::from(Vec3::new(x, Num::ZERO, z)).within_bound();
        if span.cell > Position::BOUND || !within(span.from) || !within(span.to) {
            return Err(GroundError::CoarseBound);
        }
        let [across, along] = [0, 1].map(|axis| {
            let length =
                i128::from(span.to[axis].to_bits()) - i128::from(span.from[axis].to_bits());
            Rounding::Ceiling.divide(length.abs(), i128::from(span.cell.to_bits()))
        });
        let samples = i128::from(grid.columns()) * i128::from(grid.rows());
        if across * along > samples {
            return Err(GroundError::CoarseCount);
        }
        let [Ok(across), Ok(along)] = [across, along].map(u32::try_from) else {
            return Err(GroundError::CoarseCount);
        };
        let counts = [across, along];
        let mut highest = Vec::with_capacity(counts[0] as usize * counts[1] as usize);
        for row in 0..counts[1] {
            for column in 0..counts[0] {
                let [columns, rows] = [0, 1].map(|axis| {
                    let index = [column, row][axis];
                    Self::samples_within(&grid, axis, span.square(axis, index))
                });
                let mut high = 0;
                for at in rows {
                    for across in columns.clone() {
                        high = high.max(grid.sample(across, at));
                    }
                }
                highest.push(high);
            }
        }
        Ok(Self(Some(Heights {
            grid,
            split,
            coarse: CoarseCells {
                span,
                counts,
                highest,
            },
        })))
    }

    /// The ground's height at the ground plane's `x` and `z`: the height of the triangle of its
    /// cell that holds the point, computed from the cell's samples in one step and rounded once,
    /// and past the grid the height of its nearest point.
    pub fn height(&self, x: Num, z: Num) -> Num {
        let Some(Heights { grid, split, .. }) = &self.0 else {
            return Num::ZERO;
        };
        let cell = i128::from(grid.cell().to_bits());
        let [column, row, across, along] = {
            let at = |axis: usize, value: Num, count: u32| {
                let last = cell * (i128::from(count) - 1);
                let offset = (i128::from(value.to_bits())
                    - i128::from(grid.origin()[axis].to_bits()))
                .clamp(0, last);
                let index = (offset / cell).min(i128::from(count) - 2);
                [index, offset - index * cell]
            };
            let [column, across] = at(0, x, grid.columns());
            let [row, along] = at(1, z, grid.rows());
            [column, row, across, along]
        };
        let sample = |column: i128, row: i128| {
            let index = |value: i128| u32::try_from(value).expect("an index on the grid");
            i128::from(grid.sample(index(column), index(row)))
        };
        let [h0, h1, h2, h3] = [
            sample(column, row),
            sample(column + 1, row),
            sample(column + 1, row + 1),
            sample(column, row + 1),
        ];
        // The height in steps, times the cell: each triangle's plane through its three samples.
        let scaled = match split {
            CellSplit::ZeroTwo if across >= along => {
                h0 * cell + across * (h1 - h0) + along * (h2 - h1)
            }
            CellSplit::ZeroTwo => h0 * cell + along * (h3 - h0) + across * (h2 - h3),
            CellSplit::OneThree if across + along <= cell => {
                h0 * cell + across * (h1 - h0) + along * (h3 - h0)
            }
            CellSplit::OneThree => {
                h2 * cell + (cell - across) * (h3 - h2) + (cell - along) * (h1 - h2)
            }
        };
        let bits = Rounding::NearestEven.divide(i128::from(grid.step().to_bits()) * scaled, cell);
        Num::from_bits(i64::try_from(bits).expect("a ground within the world's bound"))
    }

    /// The highest of the coarse cells a line of the ground plane from `a` to `b`, `[x, z]` each,
    /// crosses, from `a`'s cell to `b`'s by Bresenham's walk over the cells' indices, as Zero
    /// Hour's `PartitionManager::iterateCellsAlongLine` walks them, a cell off the coarse cells
    /// skipped; none when it crosses none, and 0 on a flat ground.
    pub fn highest_along(&self, a: [Num; 2], b: [Num; 2]) -> Option<Num> {
        let Some(Heights { grid, coarse, .. }) = &self.0 else {
            return Some(Num::ZERO);
        };
        let [start, end] =
            [a, b].map(|point| [0, 1].map(|axis| coarse.span.index(axis, point[axis])));
        let mut highest: Option<u8> = None;
        let counts = coarse.counts.map(i64::from);
        Self::walk(start, end, counts, |at| {
            if let Some(cell) = coarse.highest_at(at) {
                highest = Some(highest.map_or(cell, |high| high.max(cell)));
            }
        });
        highest.map(|steps| {
            grid.step()
                .checked_mul_int(steps.into())
                .expect("a grid's highest sample lies within the world's bound")
        })
    }

    /// Calls `visit` with each cell of Bresenham's walk from the cell `start` to `end`, both
    /// included, as the original's: the axis of the larger delta steps each time, the other when
    /// the numerator, from half the denominator, passes it. Only the cells whose index on the
    /// larger delta's axis lies in `0..counts` are visited, each reached in closed form, so a
    /// walk costs the cells it crosses and not the length of its line.
    fn walk(start: [i64; 2], end: [i64; 2], counts: [i64; 2], mut visit: impl FnMut([i64; 2])) {
        let delta = [0, 1].map(|axis| i128::from(end[axis]).abs_diff(i128::from(start[axis])));
        let step = [0, 1].map(|axis| if end[axis] >= start[axis] { 1 } else { -1 });
        let major = usize::from(delta[0] < delta[1]);
        let minor = 1 - major;
        let [denominator, add] = [delta[major], delta[minor]]
            .map(|value| i128::try_from(value).expect("a walk's delta within the world's bound"));
        // The steps that keep the major index in `0..counts`, `step` either way.
        let origin = i128::from(start[major]);
        let [low, high] = [-origin, i128::from(counts[major]) - 1 - origin];
        let [low, high] = if step[major] > 0 {
            [low, high]
        } else {
            [-high, -low]
        };
        for taken in low.max(0)..=high.min(denominator) {
            // After `taken` steps, the numerator passed the denominator once per minor step.
            let minor_steps = (denominator / 2 + taken * add)
                .checked_div(denominator)
                .unwrap_or(0);
            let mut at = [0; 2];
            for (axis, steps) in [(major, taken), (minor, minor_steps)] {
                let moved = i128::from(start[axis]) + i128::from(step[axis]) * steps;
                at[axis] = i64::try_from(moved).expect("a walk's cell between its ends");
            }
            visit(at);
        }
    }

    /// The indices of the samples on `axis` whose coordinates lie in `square`, both ends
    /// included, clamped to the grid.
    fn samples_within(
        grid: &HeightGrid,
        axis: usize,
        [low, high]: [Num; 2],
    ) -> RangeInclusive<u32> {
        let origin = i128::from(grid.origin()[axis].to_bits());
        let cell = i128::from(grid.cell().to_bits());
        let count = i128::from([grid.columns(), grid.rows()][axis]);
        let first = Rounding::Ceiling
            .divide(i128::from(low.to_bits()) - origin, cell)
            .clamp(0, count - 1);
        let last = Rounding::Floor
            .divide(i128::from(high.to_bits()) - origin, cell)
            .clamp(0, count - 1);
        let index = |value: i128| u32::try_from(value).expect("an index on the grid");
        index(first)..=index(last)
    }
}

impl CoarseSpan {
    /// The index on `axis` of the coarse cell that holds `value`, counted from `from` toward `to`,
    /// a point on a cell's edge in the cell it starts.
    fn index(&self, axis: usize, value: Num) -> i64 {
        let offset = i128::from(value.to_bits()) - i128::from(self.from[axis].to_bits());
        let toward = if self.to[axis] >= self.from[axis] {
            offset
        } else {
            -offset
        };
        let index = Rounding::Floor.divide(toward, i128::from(self.cell.to_bits()));
        i64::try_from(index).expect("a coarse cell's index within the world's bound")
    }

    /// The low and high coordinates on `axis` of the coarse cells of index `index`.
    fn square(&self, axis: usize, index: u32) -> [Num; 2] {
        let edge = |count: u32| {
            let length = self
                .cell
                .checked_mul_int(count.into())
                .expect("a coarse cell on the grid");
            if self.to[axis] >= self.from[axis] {
                self.from[axis] + length
            } else {
                self.from[axis] - length
            }
        };
        let [near, far] = [edge(index), edge(index + 1)];
        [near.min(far), near.max(far)]
    }
}

impl CoarseCells {
    /// The highest sample, in steps, of the coarse cell at the indices `at`; none off the cells.
    fn highest_at(&self, [column, row]: [i64; 2]) -> Option<u8> {
        let column = u32::try_from(column)
            .ok()
            .filter(|&column| column < self.counts[0])?;
        let row = u32::try_from(row)
            .ok()
            .filter(|&row| row < self.counts[1])?;
        Some(self.highest[row as usize * self.counts[0] as usize + column as usize])
    }
}

#[cfg(test)]
mod tests;
