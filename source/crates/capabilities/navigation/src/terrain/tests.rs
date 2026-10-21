use campfire_geometry::internals::KernelScene;
use campfire_geometry::{Bounds, Grid};
use campfire_math::{Num, Vec3};
use campfire_units::{Layer, Wall};

use crate::terrain::WallGrid;

#[test]
fn the_walls_split_a_scene_into_two_lanes_and_a_jungle() {
    // Half side 60 m: the walls stand at x from -32 to -28 and from 28 to 32, each from
    // z = -50 to -5 and from 5 to 50, on a grid of 1 m cells over 120 m square.
    let edge = Num::int(60);
    let grid = Grid::new(Num::ONE, Bounds::new([-edge; 2], [edge; 2]).unwrap()).unwrap();
    let walls: Vec<Wall> = KernelScene::walls(60)
        .into_iter()
        .map(|area| Wall {
            layer: Layer::FIRST,
            area,
        })
        .collect();
    let terrain = WallGrid::new(&grid, &walls);
    let blocked = terrain.blocked(Layer::FIRST).unwrap();
    let at = |x: i64, z: i64| {
        let position = Vec3::new(Num::int(x) + Num::HALF, Num::ZERO, Num::int(z) + Num::HALF);
        let cell = grid.cell_of(position).unwrap();
        blocked[cell / 64] & (1 << (cell % 64)) != 0
    };
    // Inside each of the four pieces.
    for (x, z) in [(-30, -20), (-30, 20), (30, -20), (30, 20)] {
        assert!(at(x, z), "({x}, {z}) is walled");
    }
    // Just past each face, the lanes, the jungle, the gaps and the ends where all meet.
    for (x, z) in [
        (-33, -20),
        (-28, -20),
        (27, 20),
        (32, 20),
        (-45, 0),
        (0, 0),
        (45, 0),
        (-30, 0),
        (30, 0),
        (-30, 51),
        (30, -52),
    ] {
        assert!(!at(x, z), "({x}, {z}) is open");
    }
}
