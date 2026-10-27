use serde::{Deserialize, Serialize};

/// Cells of a vision grid, by their places on it, a bit for each, 64 to a word; none before its
/// first write.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CellBits(Vec<u64>);

impl CellBits {
    /// `cells` cells, none of them held.
    pub(crate) fn empty(cells: usize) -> Self {
        Self(vec![0; cells.div_ceil(64)])
    }

    /// Whether it holds the cell at `cell`; not past its words.
    pub fn holds(&self, cell: usize) -> bool {
        self.0
            .get(cell / 64)
            .is_some_and(|word| (word >> (cell % 64)) & 1 != 0)
    }

    /// Its words, the first cells' first.
    pub fn words(&self) -> &[u64] {
        &self.0
    }

    /// Whether it holds no word, as before its first write.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Holds the cells of `words` in place of its own; whether a cell changed.
    pub(crate) fn set(&mut self, words: &[u64]) -> bool {
        if self.0 == words {
            return false;
        }
        self.0.clear();
        self.0.extend_from_slice(words);
        true
    }

    /// Holds the cells of `words` too, of as many words as it holds; whether a cell was new.
    pub(crate) fn add(&mut self, words: &[u64]) -> bool {
        debug_assert_eq!(self.0.len(), words.len(), "cells of one grid");
        let mut new = false;
        for (word, &more) in self.0.iter_mut().zip(words) {
            new |= more & !*word != 0;
            *word |= more;
        }
        new
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::cell_bits::CellBits;

    impl CellBits {
        /// `cells` cells, those of `held` held.
        pub fn of_cells(cells: usize, held: &[usize]) -> Self {
            let mut bits = Self::empty(cells);
            for &cell in held {
                bits.0[cell / 64] |= 1 << (cell % 64);
            }
            bits
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_hold_their_bits_and_tell_a_change() {
        // Cells 1 and 64 of 70, the second word's first bit.
        let mut cells = CellBits::empty(70);
        assert_eq!(cells.words(), [0, 0]);
        assert!(cells.set(&[0b10, 1]));
        assert!(!cells.set(&[0b10, 1]));
        assert_eq!(
            [0, 1, 63, 64, 65, 200].map(|cell| cells.holds(cell)),
            [false, true, false, true, false, false]
        );
        // Adding cells 1 and 2 adds 2 alone; adding them again adds none.
        assert!(cells.add(&[0b110, 0]));
        assert!(!cells.add(&[0b110, 1]));
        assert_eq!(cells.words(), [0b110, 1]);
        assert!(CellBits::default().is_empty() && !cells.is_empty());
    }
}
