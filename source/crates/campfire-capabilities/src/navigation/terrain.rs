use crate::geometry::grid::Grid;
use crate::navigation::wall::Wall;
use crate::units::layer::Layer;

/// The cells of the pathing grid the map's walls block, layer by layer, one bit a cell: derived
/// from the map once, as the walls never change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Terrain {
    /// Words of cells a layer.
    words: usize,
    /// Each layer's blocked cells, from the first layer to the last a wall stands on.
    blocked: Vec<u64>,
}

impl Terrain {
    /// The cells of `grid` that `walls` block.
    pub(crate) fn new(grid: &Grid, walls: &[Wall]) -> Terrain {
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
            wall.area
                .cells(grid, |cell| cells[cell / 64] |= 1 << (cell % 64));
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
