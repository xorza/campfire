use crate::values::grid::Grid;
use crate::values::polygon::Polygon;

/// The map's brush on the vision grid: each cell's brush, the first the map lists whose area holds
/// the cell's center, and each brush's cells, one bit a cell. Derived from the map once, as brush
/// never changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BrushMap {
    /// Words of cells a bitmap.
    words: usize,
    /// Each cell's brush, counted from 1, or 0 for none.
    of_cell: Vec<u32>,
    /// The cells of any brush, then each brush's, brush after brush.
    cells: Vec<u64>,
}

/// The cells a unit does not reveal: those of every brush but the one it stands in.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Hidden<'a> {
    any: &'a [u64],
    own: Option<&'a [u64]>,
}

impl BrushMap {
    /// The brush of `areas`, in the map's order, on `grid`; none for no area.
    pub(crate) fn new(grid: &Grid, areas: &[Polygon]) -> BrushMap {
        if areas.is_empty() {
            return BrushMap::default();
        }
        let words = grid.cells().div_ceil(64);
        let mut of_cell = vec![0; grid.cells()];
        let mut cells = vec![0; words * (areas.len() + 1)];
        for (at, area) in areas.iter().enumerate() {
            let brush = u32::try_from(at + 1).expect("a map's brush count fits u32");
            let (any, own) = cells.split_at_mut(words * (at + 1));
            let own = &mut own[..words];
            area.cells(grid, |cell| {
                if of_cell[cell] == 0 {
                    of_cell[cell] = brush;
                    let bit = 1 << (cell % 64);
                    any[cell / 64] |= bit;
                    own[cell / 64] |= bit;
                }
            });
        }
        BrushMap {
            words,
            of_cell,
            cells,
        }
    }

    /// The cells a unit standing in `cell` does not reveal; none on a map with no brush.
    pub(crate) fn hidden_from(&self, cell: usize) -> Option<Hidden<'_>> {
        let any = self.cells.get(..self.words).filter(|_| self.words > 0)?;
        let own = match self.of_cell[cell] {
            0 => None,
            brush => {
                let at = usize::try_from(brush).expect("a brush of the map") * self.words;
                Some(&self.cells[at..at + self.words])
            }
        };
        Some(Hidden { any, own })
    }
}

impl Hidden<'_> {
    /// The bits of word `at` it hides.
    pub(crate) fn word(&self, at: usize) -> u64 {
        self.any[at] & !self.own.map_or(0, |own| own[at])
    }
}
