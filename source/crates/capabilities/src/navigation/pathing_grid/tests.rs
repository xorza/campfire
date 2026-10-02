use campfire_math::Vec3;
use campfire_sim::{IdAllocator, Position};

use super::*;
use crate::units::layer::Layer;
use crate::values::bounds::Bounds;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn half() -> Num {
    Num::from_bits(1 << 23)
}

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
    let bounds = Bounds::new([num(-3), num(-3)], [num(3), num(3)]).unwrap();
    PathingGrid::new(
        Grid::new(num(1), bounds).unwrap(),
        vec![air(), ground(Num::ONE), ground(half()), ground(half())],
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
        radius: Num::ONE,
        layer: Layer::FIRST,
    };
    follow(&mut grid, &mut index, &[tower]);
    let small = ["......", "......", "..##..", "..##..", "......", "......"];
    let large = ["......", "..##..", ".####.", ".####.", "..##..", "......"];
    assert_eq!(drawn(&grid, ground(half())), small);
    assert_eq!(drawn(&grid, ground(Num::ONE)), large);

    // A post of 0.5 m at (2, −1.5), and one at (−2.5, 2.5), for a walker of 0.5 m, closer
    // than 1 m: the post blocks the centers (1.5, −1.5) and (2.5, −1.5), 0.5 m off, and not
    // (1.5, −0.5), √1.25 ≈ 1.12 m off. The corner post blocks its own cell; (−1.5, 2.5) is
    // exactly 1 m off, on the edge, and open.
    let post = IndexedBody {
        id: ids.allocate(),
        at: at(num(2), -(num(1) + half())),
        radius: half(),
        layer: Layer::FIRST,
    };
    let corner = IndexedBody {
        id: ids.allocate(),
        at: at(-(num(2) + half()), num(2) + half()),
        radius: half(),
        layer: Layer::FIRST,
    };
    follow(&mut grid, &mut index, &[tower, post, corner]);
    let small = ["......", "....##", "..##..", "..##..", "......", "#....."];
    assert_eq!(drawn(&grid, ground(half())), small);
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
    assert_eq!(drawn(&grid, ground(half())), small);
    assert_eq!(drawn(&grid, ground(Num::ONE)), large);
    let under = ["......", "..##..", ".####.", ".####.", "..##..", "......"];
    assert_eq!(drawn(&grid, air()), under);

    // Without the tower, only the others block, and the post still blocks the cells it shares
    // with the tower.
    follow(&mut grid, &mut index, &[post, corner]);
    let small = ["......", "....##", "......", "......", "......", "#....."];
    assert_eq!(drawn(&grid, ground(half())), small);
    let large = ["....##", "....##", "....##", "......", "##....", "##...."];
    assert_eq!(drawn(&grid, ground(Num::ONE)), large);

    // The post moves a meter along z, to (2, −0.5).
    let moved = IndexedBody {
        at: at(num(2), -half()),
        ..post
    };
    follow(&mut grid, &mut index, &[tower, moved, corner]);
    assert_eq!(drawn(&grid, air()), ["......"; 6]);
    follow(&mut grid, &mut index, &[]);
    assert_eq!(drawn(&grid, ground(Num::ONE)), ["......"; 6]);
}
