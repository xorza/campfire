use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use campfire_geometry::Bounds;
use campfire_math::Num;
use campfire_navigation::WallData;
use campfire_units::MapPoint;

/// A map's cliffs as the pathfinder blocks them (12-zero-hour.md, Cliffs are walls): each cliff
/// cell of the game's, and each cell of the eight round one, as `Pathfinder::classifyMap` grows
/// them, merged row by row into rectangles of whole cells, each a run of one row's cells and the
/// same run of the rows after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliffWalls {
    rects: Vec<CellRect>,
}

/// A rectangle of the game's cells, its columns and its rows, both ends in.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CellRect {
    columns: RangeInclusive<usize>,
    rows: RangeInclusive<usize>,
}

/// Where the game's cells lie on the engine's axes: the engine's point of the game's sample
/// `[0, rows − 1]`, the north-west corner of its heights, a cell's side, and the game's rows of
/// samples, which the engine's `z` runs back along.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CellFrame {
    pub(crate) north_west: [Num; 2],
    pub(crate) cell: Num,
    pub(crate) rows: usize,
}

impl CliffWalls {
    /// The walls of `cliffs`, a cell each, the game's row after row of `columns` cells.
    pub(crate) fn of(cliffs: &[bool], columns: usize) -> Self {
        debug_assert!(columns > 0 && cliffs.len().is_multiple_of(columns));
        let rows = cliffs.len() / columns;
        let grown: Vec<bool> = (0..cliffs.len())
            .map(|at| {
                let (column, row) = (at % columns, at / columns);
                let near = |at: usize, len: usize| at.saturating_sub(1)..=(at + 1).min(len - 1);
                near(row, rows)
                    .any(|row| near(column, columns).any(|column| cliffs[row * columns + column]))
            })
            .collect();
        // Each open rectangle by its run of columns, the rows it holds so far.
        let mut open: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        let mut rects = Vec::new();
        for (row, cells) in grown.chunks_exact(columns).enumerate() {
            let mut runs = Vec::new();
            let mut at = 0;
            while at < columns {
                if cells[at] {
                    let start = at;
                    while at < columns && cells[at] {
                        at += 1;
                    }
                    runs.push((start, at - 1));
                } else {
                    at += 1;
                }
            }
            let mut held = BTreeMap::new();
            for run in runs {
                held.insert(run, open.remove(&run).unwrap_or(row));
            }
            rects.extend(open.into_iter().map(|((first, last), from)| CellRect {
                columns: first..=last,
                rows: from..=row - 1,
            }));
            open = held;
        }
        rects.extend(open.into_iter().map(|((first, last), from)| CellRect {
            columns: first..=last,
            rows: from..=rows - 1,
        }));
        rects.sort_by_key(|rect| (*rect.rows.start(), *rect.columns.start()));
        Self { rects }
    }

    /// Each rectangle as a wall on the engine's axes, its cells placed by `frame`, cut to
    /// `bounds`, which no cell past them reaches; none of one wholly past them.
    pub(crate) fn walls(&self, frame: CellFrame, bounds: Bounds) -> Vec<WallData> {
        let (low, high) = (bounds.min(), bounds.max());
        self.rects
            .iter()
            .filter_map(|rect| {
                // The game's cell `[x, y]` spans its samples `x` to `x + 1` and `y` to `y + 1`;
                // its sample `y` lies `rows − 1 − y` cells along the engine's `z` from the north.
                let along = |cells: usize| {
                    frame.cell * i64::try_from(cells).expect("a map's cells fit i64")
                };
                let x = [
                    frame.north_west[0] + along(*rect.columns.start()),
                    frame.north_west[0] + along(*rect.columns.end() + 1),
                ];
                let back =
                    |sample: usize| frame.north_west[1] + along(frame.rows - 1) - along(sample);
                let z = [back(*rect.rows.end() + 1), back(*rect.rows.start())];
                let cut =
                    |[from, to]: [Num; 2], axis: usize| [from.max(low[axis]), to.min(high[axis])];
                let ([x0, x1], [z0, z1]) = (cut(x, 0), cut(z, 1));
                (x0 < x1 && z0 < z1).then(|| WallData {
                    layer: None,
                    points: [[x0, z0], [x1, z0], [x1, z1], [x0, z1]]
                        .map(MapPoint::Ground)
                        .to_vec(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cliffs_grow_a_ring_and_merge_row_by_row() {
        // 7 × 4 cells, cliffs at [1, 1] and [5, 1]: each grows over the 3 × 3 cells round it, so
        // rows 0 to 2 each hold the runs 0 to 2 and 4 to 6, two rectangles of three rows.
        let at = |cells: &[[usize; 2]]| {
            let mut cliffs = vec![false; 7 * 4];
            for &[x, y] in cells {
                cliffs[y * 7 + x] = true;
            }
            CliffWalls::of(&cliffs, 7)
        };
        let rect = |columns, rows| CellRect { columns, rows };
        assert_eq!(
            at(&[[1, 1], [5, 1]]).rects,
            [rect(0..=2, 0..=2), rect(4..=6, 0..=2)]
        );
        // A third at [3, 2] grows over columns 2 to 4 of rows 1 to 3: row 0 keeps its two runs,
        // rows 1 and 2 are one run of every column, and row 3 the run 2 to 4.
        let walls = at(&[[1, 1], [5, 1], [3, 2]]);
        assert_eq!(
            walls.rects,
            [
                rect(0..=2, 0..=0),
                rect(4..=6, 0..=0),
                rect(0..=6, 1..=2),
                rect(2..=4, 3..=3),
            ]
        );
        assert_eq!(at(&[]).rects, []);

        // On a frame of 10 m cells whose north-west sample is at (−10, −10), of 4 rows of samples:
        // row `y`'s cells span z from −10 + 30 − 10 (y + 1) = 10 − 10 y to 20 − 10 y, and column
        // `x`'s x from −10 + 10 x. In bounds from (−10, −20) to (40, 10), row 0's runs, z 10 to
        // 20, lie wholly past them, so they are none; the run of every column of rows 1 and 2,
        // x −10 to 60 and z −10 to 10, is cut at x 40; the run 2 to 4 of row 3 spans x 10 to 40
        // and z −20 to −10.
        let frame = CellFrame {
            north_west: [Num::int(-10), Num::int(-10)],
            cell: Num::int(10),
            rows: 4,
        };
        let bounds =
            Bounds::new([Num::int(-10), Num::int(-20)], [Num::int(40), Num::int(10)]).unwrap();
        let square = |[x0, z0, x1, z1]: [i64; 4]| {
            [[x0, z0], [x1, z0], [x1, z1], [x0, z1]]
                .map(|[x, z]| MapPoint::Ground([Num::int(x), Num::int(z)]))
                .to_vec()
        };
        let points: Vec<_> = walls
            .walls(frame, bounds)
            .into_iter()
            .map(|wall| (wall.layer, wall.points))
            .collect();
        assert_eq!(
            points,
            [
                (None, square([-10, -10, 40, 10])),
                (None, square([10, -20, 40, -10])),
            ]
        );
    }
}
