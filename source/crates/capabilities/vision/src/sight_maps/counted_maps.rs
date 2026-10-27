use std::ops::Range;

use campfire_geometry::RunChange;

use crate::brush_map::Hidden;

/// Maps of the same cells, each a bitmap and the count of the sights on each of its cells: a
/// cell's bit is set while its count is above 0 (04-capabilities/vision.md, A stage that costs
/// what changed).
#[derive(Debug, Default)]
pub(crate) struct CountedMaps {
    cells: usize,
    /// The words of one bitmap.
    words: usize,
    maps: usize,
    /// Each map's bitmap, map after map.
    bits: Vec<u64>,
    /// Each map's counts, map after map: 4 bytes, as a count of 2 would let a script's reveals on
    /// one cell pass 65,535, where 2³² of them would not fit in memory.
    counts: Vec<u32>,
}

impl CountedMaps {
    /// `maps` empty maps of `cells` cells.
    pub(crate) fn reset(&mut self, cells: usize, maps: usize) {
        self.cells = cells;
        self.words = cells.div_ceil(64);
        self.maps = maps;
        self.bits.clear();
        self.bits.resize(self.words * maps, 0);
        self.counts.clear();
        self.counts.resize(cells * maps, 0);
    }

    /// Adds an empty map, and gives its number.
    pub(crate) fn push(&mut self) -> usize {
        self.bits.resize(self.bits.len() + self.words, 0);
        self.counts.resize(self.counts.len() + self.cells, 0);
        self.maps += 1;
        self.maps - 1
    }

    /// Counts a sight on `cells` of `map`, a run that is not empty, but those `hidden` holds,
    /// once more when it `Gained` them, once less when it `Lost` them, and sets or clears the bit
    /// of each cell whose count leaves or reaches 0. Cell by cell, as a moved sight's runs are
    /// a cell or two.
    #[inline]
    pub(crate) fn count(
        &mut self,
        map: usize,
        cells: Range<usize>,
        hidden: Option<Hidden<'_>>,
        change: RunChange,
    ) {
        debug_assert!(!cells.is_empty() && cells.end <= self.cells && map < self.maps);
        let (bits, counts) = (map * self.words, map * self.cells);
        for cell in cells {
            let bit = 1 << (cell % 64);
            if hidden.is_some_and(|hidden| hidden.word(cell / 64) & bit != 0) {
                continue;
            }
            let count = &mut self.counts[counts + cell];
            let word = &mut self.bits[bits + cell / 64];
            match change {
                RunChange::Gained => {
                    if *count == 0 {
                        *word |= bit;
                    }
                    *count += 1;
                }
                RunChange::Lost => {
                    *count -= 1;
                    if *count == 0 {
                        *word &= !bit;
                    }
                }
            }
        }
    }

    /// The words of `map`'s bitmap.
    pub(crate) fn bits(&self, map: usize) -> &[u64] {
        &self.bits[map * self.words..][..self.words]
    }

    /// Whether `map` holds a cell of `cells`, a run, a word at a time.
    pub(crate) fn any(&self, map: usize, cells: Range<usize>) -> bool {
        if cells.is_empty() {
            return false;
        }
        let bits = &self.bits[map * self.words..][..self.words];
        (cells.start / 64..=(cells.end - 1) / 64)
            .any(|word| bits[word] & word_bits(&cells, word) != 0)
    }
}

/// The bits of word `word` that `cells`, a run that is not empty, holds.
const fn word_bits(cells: &Range<usize>, word: usize) -> u64 {
    let mut bits = u64::MAX;
    if word == cells.start / 64 {
        bits &= u64::MAX << (cells.start % 64);
    }
    if word == (cells.end - 1) / 64 {
        bits &= u64::MAX >> (63 - (cells.end - 1) % 64);
    }
    bits
}

#[cfg(test)]
mod tests {
    use crate::sight_maps::counted_maps::CountedMaps;

    impl CountedMaps {
        /// Whether every bit and every count is 0.
        pub(crate) fn is_clear(&self) -> bool {
            self.bits.iter().all(|&word| word == 0) && self.counts.iter().all(|&count| count == 0)
        }
    }
}
