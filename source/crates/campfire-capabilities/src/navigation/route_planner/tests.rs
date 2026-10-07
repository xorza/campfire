use std::cmp::Reverse;
use std::collections::BinaryHeap;

use campfire_math::{Num, Vec3};
use campfire_sim::IdAllocator;

use campfire_common::SegmentSeed;
use campfire_math::{RngSource, RngStream};

use super::*;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::terrain::Terrain;
use crate::navigation::walker::Walker;
use crate::units::layer::Layer;
use crate::values::bounds::Bounds;
use crate::values::shape::Shape;

/// The point `(x, z)` in quarters of a meter.
fn at(x: i64, z: i64) -> Position {
    let quarter = |value: i64| Num::from_bits(value << (Num::FRAC_BITS - 2));
    Position::new(Vec3::new(quarter(x), Num::ZERO, quarter(z))).unwrap()
}

/// A walker of 0.25 m on the first layer.
fn walker() -> Walker {
    Walker {
        layer: Layer::FIRST,
        radius: Num::QUARTER,
    }
}

/// A grid and the posts that block its cells.
#[derive(Debug)]
struct Walled {
    grid: PathingGrid,
    statics: BodyIndex,
}

/// 1 m cells from the origin, rows from z = 0, with a post of 0.25 m on the center of each
/// `#`, for walkers of 0.25 m: a post blocks its own cell, whose center is on it, and not its
/// neighbors', 1 m off; the walker's center keeps 0.5 m from it.
fn walled(rows: &[&str]) -> Walled {
    let quarter = Num::QUARTER;
    let size = |count: usize| Num::from_int(i64::try_from(count).unwrap()).unwrap();
    let bounds = Bounds::new([Num::ZERO; 2], [size(rows[0].len()), size(rows.len())]);
    let mut grid = PathingGrid::new(
        Grid::new(Num::ONE, bounds.unwrap()).unwrap(),
        vec![walker()],
        &Terrain::default(),
    );
    let mut ids = IdAllocator::default();
    let mut posts = Vec::new();
    for (row, line) in rows.iter().enumerate() {
        for (column, mark) in line.chars().enumerate() {
            if mark == '#' {
                let center = |index: usize| i64::try_from(4 * index + 2).unwrap();
                posts.push(IndexedBody {
                    id: ids.allocate(),
                    at: at(center(column), center(row)),
                    shape: Shape::Circle(quarter),
                    layer: Layer::FIRST,
                });
            }
        }
    }
    let mut statics = BodyIndex::new(quarter);
    statics.update(&posts);
    grid.update(&statics);
    Walled { grid, statics }
}

#[test]
fn a_route_goes_through_the_gap_and_never_across_a_blocked_corner() {
    // A wall down column 3 over rows 0 to 2, then down column 4 over rows 3 and 4. Cells (3, 2)
    // and (4, 3) meet at a corner, which a diagonal step from (3, 3) to (4, 2) would cut: the
    // route must go round through row 5.
    let Walled { grid, statics } = walled(&[
        "...#....", "...#....", "...#....", "....#...", "....#...", "........",
    ]);
    let clearance = grid.clearance(walker());
    let walkable = Walkable {
        clearance,
        statics: &statics,
        short: None,
    };
    let mut planner = RoutePlanner::new(clearance.grid());
    let mut waypoints = Vec::new();
    let (start, goal) = (at(2, 10), at(29, 11));
    let first = planner.plan(walkable, start, goal, &mut waypoints);
    // From (0, 2) three diagonals to (3, 5), 42; a step to (4, 5), 10; a step to (5, 5), as a
    // diagonal to (5, 4) would pass the blocked (4, 4), 10; then diagonals to (6, 4) and
    // (7, 3), 28, and a step to (7, 2), 10: 100. Across the corner it would be (1, 3), (2, 3),
    // (3, 3) and (4, 2), 14 + 10 + 10 + 14, and 30 on to (7, 2): 78.
    assert_eq!(first.cost, 100);
    assert!(first.reached);
    // The route runs (4, 5), (5, 5), (6, 4), (7, 3), (7, 2): among the cells of total 100 the
    // one of lower estimate goes first, (5, 5) at 38, then (6, 4) at 24, then (7, 3) at 10.
    // From the start, (0.5, 2.5), the line to (4.5, 5.5), along (4, 3), passes the post at
    // (4.5, 4.5) |4·2 − 3·4| / 5 = 0.8 m off, and the line to (5.5, 5.5), along (5, 3),
    // |5·2 − 3·4| / √34 ≈ 0.34 m off, less than 0.5. From (4.5, 5.5) the line to the goal,
    // (7.25, 2.75), along (1, −1), passes that post √0.5 ≈ 0.71 m off, and the post at
    // (4.5, 3.5) √2 m off.
    assert_eq!(waypoints, [at(18, 22), goal]);

    // The same route from a planner that planned others, and from a new one.
    let mut other = Vec::new();
    planner.plan(walkable, at(29, 1), at(1, 21), &mut other);
    let mut again = Vec::new();
    assert_eq!(planner.plan(walkable, start, goal, &mut again), first);
    assert_eq!(again, waypoints);
    let mut fresh = Vec::new();
    assert_eq!(
        RoutePlanner::new(clearance.grid()).plan(walkable, start, goal, &mut fresh),
        first
    );
    assert_eq!(fresh, waypoints);

    // A goal on the post at (3.5, 1.5): the nearest open centers, (2.5, 1.5) and (4.5, 1.5),
    // are a meter off, and (2, 1) has the lower number. The search expands (0, 2), then (1, 1)
    // of total 14 + 10, before (1, 2) of 10 + 14, then (2, 1) of 24 + 0. Its work: the test of
    // the goal, the one region, through row 5, and its 48 cells scanned for the nearest, the
    // 3 cells expanded, and one line tested, from the start straight to (2.5, 1.5): 54.
    let blocked = planner.plan(walkable, start, at(14, 6), &mut waypoints);
    assert_eq!(
        blocked,
        Planned {
            cost: 24,
            expanded: 3,
            work: 54,
            reached: false,
        }
    );
    assert_eq!(waypoints, [at(10, 6)]);

    // A goal at (3.5, 3.25) in the open cell (3, 3), past the corner of the wall: a
    // walker may stand there, 0.75 m from (3.5, 2.5) and 1.03 m from (4.5, 3.5).
    let corner = planner.plan(walkable, start, at(14, 13), &mut waypoints);
    assert!(corner.reached);
    assert_eq!(waypoints, [at(14, 13)]);

    // A goal in a blocked cell that a walker may stand on: (3.5, 0.5) is a post, but (3.85,
    // 0.0) is clear of it by √(0.35² + 0.5²) ≈ 0.61 m, so the route ends on it.
    let edge = Position::new(Vec3::new(
        Num::from_bits((385 << Num::FRAC_BITS) / 100),
        Num::ZERO,
        Num::ZERO,
    ))
    .unwrap();
    let tucked = planner.plan(walkable, start, edge, &mut waypoints);
    assert!(tucked.reached);
    assert_eq!(waypoints.last(), Some(&edge));

    // Behind a wall with no gap, down column 4: the regions tell before the search that
    // (6.5, 1.5) is out of reach, and the route goes to the nearest cell it reaches, (3, 1), a
    // meter from the wall. The search expands (0, 1), (1, 1) and (2, 1), each of total 30, then
    // (3, 1): 4 cells, where a search with no regions spreads over all 12 open cells left of
    // the wall to learn that it fails. Its work: the test of the goal, the 2 regions and the
    // 12 cells of the start's scanned, the 4 cells expanded, and two lines tested, to (2.5,
    // 1.5) and to (3.5, 1.5): 21.
    let Walled {
        grid: closed,
        statics: closed_statics,
    } = walled(&["....#...", "....#...", "....#..."]);
    let closed_clearance = closed.clearance(walker());
    let closed_walkable = Walkable {
        clearance: closed_clearance,
        statics: &closed_statics,
        short: None,
    };
    let mut closed_planner = RoutePlanner::new(closed_clearance.grid());
    let walled_off = closed_planner.plan(closed_walkable, at(2, 6), at(26, 6), &mut waypoints);
    assert_eq!(
        walled_off,
        Planned {
            cost: 30,
            expanded: 4,
            work: 21,
            reached: false,
        }
    );
    assert_eq!(waypoints, [at(14, 6)]);

    // A goal in the start's cell: nothing to expand past it, and no line to test past the
    // goal's.
    let here = planner.plan(walkable, start, at(3, 9), &mut waypoints);
    assert_eq!(
        here,
        Planned {
            cost: 0,
            expanded: 1,
            work: 2,
            reached: true,
        }
    );
    assert_eq!(waypoints, [at(3, 9)]);
}

/// Posts on a 9 by 7 grid, and windows of 2 cells each way around cells at its corners, edges
/// and middle.
fn scattered() -> Walled {
    walled(&[
        "#..#.##..",
        ".#...#..#",
        "..##..#..",
        "#....#.##",
        ".##.#....",
        "...#..##.",
        "#.#....#.",
    ])
}

fn windows(cells: &Grid) -> [Window; 7] {
    [0, 4, 8, 31, 54, 58, 62].map(|cell| Window::around(cells, cell, 2))
}

#[test]
fn the_nearest_open_cell_is_the_nearest_of_the_grid_or_the_window_by_every_cell() {
    // Goals at every quarter meter from (-1, -1) m to (10, 8) m, outside the grid too, over the
    // whole grid and over the windows: the rings must find what a scan of every cell a walker
    // may stand in finds, the nearest center, ties to the lower number.
    let Walled { grid, statics } = scattered();
    let clearance = grid.clearance(walker());
    let cells = clearance.grid();
    let columns = cells.columns();
    let mut planner = RoutePlanner::new(cells);
    let windows = windows(cells);
    let shorts = [None].into_iter().chain(windows.map(Some));
    for window in shorts {
        let walkable = Walkable {
            clearance,
            statics: &statics,
            short: window.map(|window| Short {
                window,
                blockers: &[],
            }),
        };
        planner.mark_blockers(walkable);
        let open = |at: usize| {
            RoutePlanner::passable(&planner.overlay, walkable, at % columns, at / columns)
        };
        for x in -4..=40 {
            for z in -4..=32 {
                let goal = at(x, z);
                let scanned = (0..columns * cells.rows())
                    .filter(|&at| open(at))
                    .map(|at| (cells.center_distance(at, goal), at))
                    .min()
                    .map(|(_, at)| at);
                assert_eq!(
                    planner.nearest_open(walkable, goal, &mut 0),
                    scanned,
                    "{window:?} {x} {z}"
                );
            }
        }
    }

    // A window with no cell a walker may stand in holds no nearest cell, and a short route in
    // it plans nothing: no waypoint, not reached. Its work: the test of the goal, and the
    // rings out from the goal's cell (0, 0) to the window's 9 cells, 1, 3 and 5 of them.
    let Walled {
        grid: walls,
        statics,
    } = walled(&["#####"; 5]);
    let clearance = walls.clearance(walker());
    let walkable = Walkable {
        clearance,
        statics: &statics,
        short: Some(Short {
            window: Window::around(clearance.grid(), 12, 1),
            blockers: &[],
        }),
    };
    let mut planner = RoutePlanner::new(clearance.grid());
    let mut waypoints = vec![at(0, 0)];
    let nowhere = planner.plan(walkable, at(10, 10), at(2, 2), &mut waypoints);
    assert_eq!(
        nowhere,
        Planned {
            cost: 0,
            expanded: 0,
            work: 10,
            reached: false,
        }
    );
    assert_eq!(waypoints, []);
}

#[test]
fn a_window_marks_exactly_its_cells_among_all_its_blockers_cells() {
    // Posts of 0.25 m to 2.5 m inside, across the edges of and outside the windows: the marks
    // of each, each blocker's runs of cells cut to it, are its cells among all their cells.
    let Walled { grid, statics } = scattered();
    let clearance = grid.clearance(walker());
    let cells = clearance.grid();
    let columns = cells.columns();
    let mut planner = RoutePlanner::new(cells);
    let mut ids = IdAllocator::default();
    let blockers =
        [(0, 0, 1), (6, 14, 4), (17, 9, 10), (30, 26, 3), (40, 30, 6)].map(|(x, z, quarters)| {
            IndexedBody {
                id: ids.allocate(),
                at: at(x, z),
                shape: Shape::Circle(Num::from_bits(quarters << (Num::FRAC_BITS - 2))),
                layer: Layer::FIRST,
            }
        });
    let radius = walker().radius;
    for window in windows(cells) {
        let walkable = Walkable {
            clearance,
            statics: &statics,
            short: Some(Short {
                window,
                blockers: &blockers,
            }),
        };
        planner.mark_blockers(walkable);
        let mut spanned = vec![false; window.cells()];
        for body in &blockers {
            cells.spans_closer(body.at, radius + body.shape.bound(), |run| {
                for cell in run {
                    let (x, z) = (cell % columns, cell / columns);
                    if window.contains(x, z) {
                        spanned[window.local(x, z)] = true;
                    }
                }
            });
        }
        let marked: Vec<bool> = (0..window.cells())
            .map(|local| planner.overlay[local / 64] & 1 << (local % 64) != 0)
            .collect();
        assert_eq!(marked, spanned, "{window:?}");
        assert!(marked.contains(&true), "{window:?}");
    }
}

/// The cheapest cost from cell `from` to each cell of `clearance`, by the moves a route takes:
/// 10 a straight step and 14 a diagonal one, a diagonal only past two open cells; `None` for a
/// cell no route from `from` reaches.
fn costs_from(clearance: Clearance<'_>, from: usize) -> Vec<Option<u32>> {
    let grid = clearance.grid();
    let (columns, rows) = (grid.columns(), grid.rows());
    let open = |x: usize, z: usize| x < columns && z < rows && clearance.open(z * columns + x);
    let mut costs = vec![None; grid.cells()];
    let mut heap = BinaryHeap::from([Reverse((0_u32, from))]);
    while let Some(Reverse((cost, cell))) = heap.pop() {
        if costs[cell].is_some() {
            continue;
        }
        costs[cell] = Some(cost);
        let (x, z) = (cell % columns, cell / columns);
        for (dx, dz) in [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ] {
            let (nx, nz) = (x.wrapping_add_signed(dx), z.wrapping_add_signed(dz));
            if !open(nx, nz) {
                continue;
            }
            let diagonal = dx != 0 && dz != 0;
            if diagonal && !(open(nx, z) && open(x, nz)) {
                continue;
            }
            let step = if diagonal { 14 } else { 10 };
            heap.push(Reverse((cost + step, nz * columns + nx)));
        }
    }
    costs
}

#[test]
fn a_route_costs_what_a_search_of_every_cell_finds() {
    // Maps of 16 by 12 cells, a post on about one cell in 6, 4 and 3, routes between the centers
    // of open cells. A goal a route reaches costs the cheapest way there; one it does not ends on
    // the cell nearest it among those it reaches, ties to the lower number, at that cell's cost,
    // and so the regions say. Every leg of a route is clear of every post, and a search expands
    // no more cells than are open.
    let source = RngSource::new(SegmentSeed::new([0; 32]));
    let mut rng = source.open(RngStream::new("posts"), 7);
    let mut draw = |bound: u64| rng.below(bound);
    for odds in [6, 4, 3] {
        let rows: Vec<String> = (0..12)
            .map(|_| {
                (0..16)
                    .map(|_| if draw(odds) == 0 { '#' } else { '.' })
                    .collect()
            })
            .collect();
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        let Walled { grid, statics } = walled(&rows);
        let clearance = grid.clearance(walker());
        let cells = clearance.grid();
        let walkable = Walkable {
            clearance,
            statics: &statics,
            short: None,
        };
        let regions = clearance.regions();
        let mut planner = RoutePlanner::new(cells);
        let open: Vec<usize> = (0..cells.cells())
            .filter(|&cell| clearance.open(cell))
            .collect();
        let mut waypoints = Vec::new();
        for &from in open.iter().step_by(7) {
            let costs = costs_from(clearance, from);
            let start = cells.center(from, Num::ZERO);
            for &to in open.iter().step_by(5) {
                let goal = cells.center(to, Num::ZERO);
                let route = planner.plan(walkable, start, goal, &mut waypoints);
                let connected = regions.reach(from).meets(regions.reach(to));
                assert_eq!(costs[to].is_some(), connected, "{rows:?} {from} {to}");
                let expected = costs[to].map_or_else(
                    || {
                        let found = (0..cells.cells()).filter(|&cell| costs[cell].is_some());
                        let nearest = found
                            .min_by_key(|&cell| (cells.center_distance(cell, goal), cell))
                            .unwrap();
                        (costs[nearest].unwrap(), false)
                    },
                    |cost| (cost, true),
                );
                assert_eq!(
                    (route.cost, route.reached),
                    expected,
                    "{rows:?} {from} {to}"
                );
                assert!(route.expanded as usize <= open.len());
                let mut at = start;
                for &next in &waypoints {
                    assert!(
                        !walkable.blocks(Segment::new(at, next)),
                        "{rows:?} {from} {to}"
                    );
                    at = next;
                }
            }
        }
    }
}
