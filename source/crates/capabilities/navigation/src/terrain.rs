use campfire_core::Layer;
use campfire_geometry::Grid;

use crate::wall::Wall;

/// The cells of the pathing grid the map's walls block, layer by layer, one bit a cell: derived
/// from the map once, as the walls never change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Terrain {
    /// Words of cells a layer.
    words: usize,
    /// Each layer's blocked cells, from the first layer to the last a wall stands on.
    blocked: Vec<u64>,
}

impl Terrain {
    /// The cells of `grid` that `walls` block.
    pub fn new(grid: &Grid, walls: &[Wall]) -> Terrain {
        let words = grid.cells().div_ceil(64);
        let layers = walls
            .iter()
            .map(|wall| wall.layer.index() + 1)
            .max()
            .unwrap_or(0);
        let mut blocked = vec![0; words * layers];
        for wall in walls {
            let layer = wall.layer.index();
            let cells = &mut blocked[layer * words..(layer + 1) * words];
            grid.polygon_cells(&wall.area, |cell| cells[cell / 64] |= 1 << (cell % 64));
        }
        Terrain { words, blocked }
    }

    /// The cells the walls of `layer` block; none for a layer with no wall.
    pub(crate) fn blocked(&self, layer: Layer) -> Option<&[u64]> {
        let at = layer.index();
        let cells = self.blocked.get(at * self.words..(at + 1) * self.words)?;
        cells.iter().any(|&word| word != 0).then_some(cells)
    }
}

#[cfg(test)]
mod tests {
    use campfire_core::Layer;
    use campfire_geometry::internals::KernelScene;
    use campfire_geometry::{Bounds, Grid};
    use campfire_math::{Num, Vec3};
    use campfire_sim::Position;

    use crate::terrain::Terrain;
    use crate::wall::Wall;

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
        let terrain = Terrain::new(&grid, &walls);
        let blocked = terrain.blocked(Layer::FIRST).unwrap();
        let at = |x: i64, z: i64| {
            let pos = Position::new(Vec3::new(
                Num::int(x) + Num::HALF,
                Num::ZERO,
                Num::int(z) + Num::HALF,
            ));
            let cell = grid.cell_of(pos.unwrap()).unwrap();
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
}
