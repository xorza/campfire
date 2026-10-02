use std::ops::Range;

/// The cells each vision group sees, and those its detectors see, as bitmaps kept between ticks:
/// a tick clears only the words the tick before set, so it costs what the units see, not the
/// map's size times the groups. A group has a detection bitmap only once a unit of it detects.
#[derive(Debug, Default)]
pub(crate) struct SightMaps {
    /// The words of one bitmap.
    words: usize,
    /// Each group's bitmap, group after group.
    revealed: Vec<u64>,
    /// Each group's place among the detection bitmaps, once it has one.
    detection: Vec<Option<usize>>,
    detected: Vec<u64>,
    /// The words of `revealed`, then of `detected`, that this tick set, to clear the next.
    set_revealed: Vec<usize>,
    set_detected: Vec<usize>,
}

impl SightMaps {
    /// Empty bitmaps of `cells` cells for `groups` groups, none detecting.
    pub(crate) fn reset(&mut self, cells: usize, groups: usize) {
        self.words = cells.div_ceil(64);
        self.revealed.clear();
        self.revealed.resize(self.words * groups, 0);
        self.detection.clear();
        self.detection.resize(groups, None);
        self.detected.clear();
        self.set_revealed.clear();
        self.set_detected.clear();
    }

    /// Clears what the tick before revealed and detected.
    pub(crate) fn begin_tick(&mut self) {
        for &at in &self.set_revealed {
            self.revealed[at] = 0;
        }
        for &at in &self.set_detected {
            self.detected[at] = 0;
        }
        self.set_revealed.clear();
        self.set_detected.clear();
    }

    /// Reveals `cells`, a run that is not empty, to `group`, and to its detection too when
    /// `detects`.
    pub(crate) fn reveal(&mut self, group: usize, cells: Range<usize>, detects: bool) {
        let run = group * self.words;
        set_bits(
            &mut self.revealed,
            &mut self.set_revealed,
            run,
            cells.clone(),
        );
        if detects {
            let run = self.detection_slot(group) * self.words;
            set_bits(&mut self.detected, &mut self.set_detected, run, cells);
        }
    }

    /// The place of `group`'s detection bitmap, which it gets on its first detector.
    fn detection_slot(&mut self, group: usize) -> usize {
        if let Some(slot) = self.detection[group] {
            return slot;
        }
        let slot = self.detected.len() / self.words.max(1);
        self.detected.resize(self.detected.len() + self.words, 0);
        self.detection[group] = Some(slot);
        slot
    }

    /// Whether `group` sees `cell`: with its sight, or for a unit its tags hide, with its
    /// detection.
    pub(crate) fn sees(&self, group: usize, cell: usize, hidden: bool) -> bool {
        let (map, run) = if hidden {
            let Some(slot) = self.detection[group] else {
                return false;
            };
            (&self.detected, slot * self.words)
        } else {
            (&self.revealed, group * self.words)
        };
        map[run + cell / 64] & 1 << (cell % 64) != 0
    }
}

/// Sets the bits of `cells`, a run that is not empty, in the bitmap at `run` of `words`, and
/// notes each word it sets in `set`.
fn set_bits(words: &mut [u64], set: &mut Vec<usize>, run: usize, cells: Range<usize>) {
    debug_assert!(!cells.is_empty());
    let (first, last) = (run + cells.start / 64, run + (cells.end - 1) / 64);
    let from = u64::MAX << (cells.start % 64);
    let to = u64::MAX >> (63 - (cells.end - 1) % 64);
    if first == last {
        words[first] |= from & to;
    } else {
        words[first] |= from;
        words[first + 1..last].fill(u64::MAX);
        words[last] |= to;
    }
    set.extend(first..=last);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_bits_fills_a_run_within_one_word_and_across_words_and_notes_them() {
        let (mut words, mut set) = ([0; 3], Vec::new());
        set_bits(&mut words, &mut set, 0, 3..5);
        assert_eq!(words, [0b11000, 0, 0]);
        set_bits(&mut words, &mut set, 0, 63..64);
        assert_eq!(words, [0b11000 | 1 << 63, 0, 0]);
        assert_eq!(set, [0, 0]);
        // Bits 60 to 130: the top 4 of word 0, all of word 1, and the low 3 of word 2; at run
        // 1, word 0 of the map is word 1.
        let (mut words, mut set) = ([0; 4], Vec::new());
        set_bits(&mut words, &mut set, 1, 60..131);
        assert_eq!(words, [0, 0xF << 60, u64::MAX, 0b111]);
        assert_eq!(set, [1, 2, 3]);
    }

    #[test]
    fn a_tick_clears_only_what_the_tick_before_revealed() {
        // Two groups over 200 cells, 4 words each. Group 1 sees cells 60 to 69, across words 0
        // and 1, and detects 130; group 0 detects nothing.
        let mut maps = SightMaps::default();
        maps.reset(200, 2);
        maps.reveal(1, 60..70, false);
        maps.reveal(1, 130..131, true);
        let seen = |maps: &SightMaps, group, cell, hidden| maps.sees(group, cell, hidden);
        assert!(seen(&maps, 1, 60, false) && seen(&maps, 1, 69, false));
        assert!(!seen(&maps, 1, 59, false) && !seen(&maps, 1, 70, false));
        assert!(!seen(&maps, 0, 65, false));
        assert!(seen(&maps, 1, 130, true) && !seen(&maps, 1, 65, true));
        assert!(!seen(&maps, 0, 130, true));
        // The next tick starts clear: only the words set were cleared, and they were all set.
        maps.begin_tick();
        assert!(
            maps.revealed
                .iter()
                .chain(&maps.detected)
                .all(|&word| word == 0)
        );
        assert_eq!((maps.set_revealed.len(), maps.set_detected.len()), (0, 0));
        // Group 0 then sees a run across three words: 0 to 191.
        maps.reveal(0, 0..192, false);
        assert_eq!(maps.set_revealed, [0, 1, 2]);
        assert!(seen(&maps, 0, 191, false) && !seen(&maps, 0, 192, false));
    }
}
