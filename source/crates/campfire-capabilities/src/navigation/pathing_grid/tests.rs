use campfire_math::Vec3;
use campfire_sim::{IdAllocator, Position};

use super::*;
use crate::geometry::bounds::Bounds;
use crate::geometry::polygon::Polygon;
use crate::geometry::shape::Shape;
use crate::navigation::wall::Wall;
use crate::units::layer::Layer;
/// A walker of `radius` on the first layer.
fn ground(radius: Num) -> Walker {
    Walker {
        layer: Layer::FIRST,
        radius,
    }
}

/// A walker of 1 m on the second layer.
fn air() -> Walker {
    Walker {
        layer: Layer::new(1),
        radius: Num::ONE,
    }
}

/// Each cell of the 6 × 6 grid of 1 m cells over (−3, −3) to (3, 3), row by row from z = −3,
/// as `#` where `walker` cannot stand, `.` where it can.
fn drawn(grid: &PathingGrid, walker: Walker) -> Vec<String> {
    (0..6)
        .map(|row| {
            (0..6)
                .map(|column| {
                    if grid.clearance(walker).open(row * 6 + column) {
                        '.'
                    } else {
                        '#'
                    }
                })
                .collect()
        })
        .collect()
}

/// A grid over the 6 × 6 cells for walkers of 1 m and 0.5 m on the ground, and of 1 m in
/// the air.
fn grid() -> PathingGrid {
    let bounds = Bounds::new([Num::int(-3), Num::int(-3)], [Num::int(3), Num::int(3)]).unwrap();
    PathingGrid::new(
        Grid::new(Num::int(1), bounds).unwrap(),
        vec![
            air(),
            ground(Num::ONE),
            ground(Num::HALF),
            ground(Num::HALF),
        ],
        &Terrain::default(),
    )
}

/// Marks `grid` for `statics`, which `index` held the bodies before, and checks it against a
/// grid marked for them alone.
fn follow(grid: &mut PathingGrid, index: &mut BodyIndex, statics: &[IndexedBody]) {
    index.update(statics);
    grid.update(index);
    let (mut alone, mut fresh) = (BodyIndex::new(Num::ONE), self::grid());
    alone.update(statics);
    fresh.update(&alone);
    assert_eq!(grid.blocked, fresh.blocked, "{statics:?}");
}

#[test]
fn a_static_body_blocks_the_cells_closer_than_the_two_radii() {
    let mut grid = grid();
    let mut index = BodyIndex::new(Num::ONE);
    let mut ids = IdAllocator::default();
    let at = |x: Num, z: Num| Position::new(Vec3::new(x, Num::ZERO, z)).unwrap();
    // A tower of 1 m at the origin. Centers sit at ±0.5, ±1.5 and ±2.5. For a walker of 0.5
    // m it blocks the centers closer than 1.5 m: the four at √0.5 ≈ 0.71 m; the next, such as
    // (1.5, 0.5) at √2.5 ≈ 1.58 m, are open. For a walker of 1 m, closer than 2 m: those
    // eight too, but not the corners (1.5, 1.5) at √4.5 ≈ 2.12 m.
    let tower = IndexedBody {
        id: ids.allocate(),
        at: at(Num::ZERO, Num::ZERO),
        shape: Shape::Circle(Num::ONE),
        layer: Layer::FIRST,
    };
    follow(&mut grid, &mut index, &[tower]);
    let small = ["......", "......", "..##..", "..##..", "......", "......"];
    let large = ["......", "..##..", ".####.", ".####.", "..##..", "......"];
    assert_eq!(drawn(&grid, ground(Num::HALF)), small);
    assert_eq!(drawn(&grid, ground(Num::ONE)), large);

    // A post of 0.5 m at (2, −1.5), and one at (−2.5, 2.5), for a walker of 0.5 m, closer
    // than 1 m: the post blocks the centers (1.5, −1.5) and (2.5, −1.5), 0.5 m off, and not
    // (1.5, −0.5), √1.25 ≈ 1.12 m off. The corner post blocks its own cell; (−1.5, 2.5) is
    // exactly 1 m off, on the edge, and open.
    let post = IndexedBody {
        id: ids.allocate(),
        at: at(Num::int(2), -(Num::int(1) + Num::HALF)),
        shape: Shape::Circle(Num::HALF),
        layer: Layer::FIRST,
    };
    let corner = IndexedBody {
        id: ids.allocate(),
        at: at(-(Num::int(2) + Num::HALF), Num::int(2) + Num::HALF),
        shape: Shape::Circle(Num::HALF),
        layer: Layer::FIRST,
    };
    follow(&mut grid, &mut index, &[tower, post, corner]);
    let small = ["......", "....##", "..##..", "..##..", "......", "#....."];
    assert_eq!(drawn(&grid, ground(Num::HALF)), small);
    // For a walker of 1 m, closer than 1.5 m to the post: its own two cells, the two above
    // and the two below at √1.25 ≈ 1.12 m, such as (1.5, −0.5), which the tower blocks too;
    // (0.5, −1.5), 1.5 m off, is open. The corner post blocks (−2.5, 1.5) and (−1.5, 1.5) at
    // most √2 ≈ 1.41 m off, and (−1.5, 2.5) at 1 m.
    let large = ["....##", "..####", ".#####", ".####.", "####..", "##...."];
    assert_eq!(drawn(&grid, ground(Num::ONE)), large);
    // None of them is in the air; a cloud of 1 m at the origin is, and blocks for the air
    // walker of 1 m what the tower blocks for the ground walker of 1 m, and nothing on the
    // ground.
    assert_eq!(drawn(&grid, air()), ["......"; 6]);
    let cloud = IndexedBody {
        id: ids.allocate(),
        layer: air().layer,
        ..tower
    };
    follow(&mut grid, &mut index, &[tower, post, corner, cloud]);
    assert_eq!(drawn(&grid, ground(Num::HALF)), small);
    assert_eq!(drawn(&grid, ground(Num::ONE)), large);
    let under = ["......", "..##..", ".####.", ".####.", "..##..", "......"];
    assert_eq!(drawn(&grid, air()), under);

    // Without the tower, only the others block, and the post still blocks the cells it shares
    // with the tower.
    follow(&mut grid, &mut index, &[post, corner]);
    let small = ["......", "....##", "......", "......", "......", "#....."];
    assert_eq!(drawn(&grid, ground(Num::HALF)), small);
    let large = ["....##", "....##", "....##", "......", "##....", "##...."];
    assert_eq!(drawn(&grid, ground(Num::ONE)), large);

    // The post moves a meter along z, to (2, −0.5).
    let moved = IndexedBody {
        at: at(Num::int(2), -Num::HALF),
        ..post
    };
    follow(&mut grid, &mut index, &[tower, moved, corner]);
    assert_eq!(drawn(&grid, air()), ["......"; 6]);
    follow(&mut grid, &mut index, &[]);
    assert_eq!(drawn(&grid, ground(Num::ONE)), ["......"; 6]);
}

#[test]
fn a_wall_blocks_its_cells_and_those_a_walker_comes_closer_to_and_no_body_opens_them() {
    // A wall on the ground round the origin, from (−0.5, −0.5) to (0.5, 0.5): the four centers
    // (±0.5, ±0.5) lie on its corners, so it blocks those four cells.
    let bounds = Bounds::new([Num::int(-3), Num::int(-3)], [Num::int(3), Num::int(3)]).unwrap();
    let cells = Grid::new(Num::int(1), bounds).unwrap();
    let corner = |x: i64, z: i64| [Num::HALF * x, Num::HALF * z];
    let area = Polygon::new(vec![
        corner(-1, -1),
        corner(1, -1),
        corner(1, 1),
        corner(-1, 1),
    ]);
    let wall = Wall {
        layer: Layer::FIRST,
        area: area.unwrap(),
    };
    let terrain = Terrain::new(&cells, &[wall]);
    let walkers = vec![
        air(),
        ground(Num::ZERO),
        ground(Num::HALF),
        ground(Num::ONE),
    ];
    let mut grid = PathingGrid::new(cells, walkers, &terrain);
    // A neighbor's center lies half a meter from a blocked cell's square, and a diagonal one
    // √0.5 ≈ 0.71 m: a walker of no body and one of 0.5 m come closer to neither, and stand
    // there; one of 1 m comes closer to both, and the wall keeps it from a ring round the four.
    let walled = ["......", "......", "..##..", "..##..", "......", "......"];
    let ringed = ["......", ".####.", ".####.", ".####.", ".####.", "......"];
    let drawn_all = |grid: &PathingGrid| {
        [
            ground(Num::ZERO),
            ground(Num::HALF),
            ground(Num::ONE),
            air(),
        ]
        .map(|walker| drawn(grid, walker))
    };
    let open = ["......"; 6].map(str::to_owned).to_vec();
    let lines = |rows: [&str; 6]| rows.map(str::to_owned).to_vec();
    let expected = [lines(walled), lines(walled), lines(ringed), open.clone()];
    assert_eq!(drawn_all(&grid), expected);
    // A tower of 1 m at the origin blocks for the walker of 1 m the cells within 2 m of it, which
    // the ring holds; once it is gone, the wall's cells stay blocked.
    let mut index = BodyIndex::new(Num::ONE);
    let tower = IndexedBody {
        id: IdAllocator::default().allocate(),
        at: Position::new(Vec3::new(Num::ZERO, Num::ZERO, Num::ZERO)).unwrap(),
        shape: Shape::Circle(Num::ONE),
        layer: Layer::FIRST,
    };
    index.update(&[tower]);
    grid.update(&index);
    let small = ["......", "......", "..##..", "..##..", "......", "......"];
    assert_eq!(drawn(&grid, ground(Num::HALF)), small);
    assert_eq!(drawn(&grid, ground(Num::ONE)), ringed);
    index.update(&[]);
    grid.update(&index);
    assert_eq!(drawn_all(&grid), expected);
}

#[test]
fn the_walls_test_visits_and_finds_what_a_walk_cell_by_cell_does() {
    // Half-meter cells over 11.5 m by 8.5 m, 23 columns of 17 rows, so a column's run of rows
    // crosses a word's edge; walls of two rectangles and a triangle, for walkers of 0 to 1 m.
    let bounds = Bounds::new(
        [Num::int(-6), Num::int(-4)],
        [Num::int(5) + Num::HALF, Num::int(4) + Num::HALF],
    )
    .unwrap();
    let cells = Grid::new(Num::HALF, bounds).unwrap();
    let at = |x: i64, z: i64| {
        [
            Num::from_bits(x * Num::ONE.to_bits() / 4),
            Num::from_bits(z * Num::ONE.to_bits() / 4),
        ]
    };
    let areas = [
        vec![at(-16, -9), at(-10, -9), at(-10, 6), at(-16, 6)],
        vec![at(1, -2), at(13, -2), at(13, 3), at(1, 3)],
        vec![at(8, 8), at(17, 14), at(4, 15)],
    ];
    let walls: Vec<Wall> = areas
        .into_iter()
        .map(|points| Wall {
            layer: Layer::FIRST,
            area: Polygon::new(points).unwrap(),
        })
        .collect();
    let terrain = Terrain::new(&cells, &walls);
    let walkers = vec![ground(Num::ZERO), ground(Num::HALF), ground(Num::ONE)];
    let grid = PathingGrid::new(cells, walkers.clone(), &terrain);
    let mut state = 0x3A11_u64;
    let mut next = move |span: u64| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) % span
    };
    for case in 0..4000 {
        let walker = walkers[case % walkers.len()];
        let clearance = grid.clearance(walker);
        let mut point = || {
            let x = next(47).cast_signed() - 24;
            let z = next(35).cast_signed() - 16;
            let [x, z] = at(x, z);
            Position::new(Vec3::new(x, Num::ZERO, z)).unwrap()
        };
        // Lines along each axis and single points among them.
        let (from, other) = (point(), point());
        let to = match case % 9 {
            0 => from,
            1 => Position::new(Vec3::new(from.get().x, Num::ZERO, other.get().z)).unwrap(),
            2 => Position::new(Vec3::new(other.get().x, Num::ZERO, from.get().z)).unwrap(),
            _ => other,
        };
        let segment = Segment::new(from, to);
        let walled = clearance.walled.unwrap();
        let mut expected_work = 0;
        let expected = clearance
            .grid
            .touches(segment.start(), segment.end(), |cell| {
                expected_work += 1;
                walled[cell / 64] & 1 << (cell % 64) != 0
            });
        let mut work = 0;
        assert_eq!(
            clearance.walled(segment, &mut work),
            expected,
            "{segment:?}"
        );
        assert_eq!(work, expected_work, "{segment:?}");
    }
}
