use std::mem;
use std::ops::Range;

use crate::vision::brush_map::Hidden;

/// The cells each vision group sees, and those its detectors see, as bitmaps kept between ticks:
/// a tick clears only the words the tick before set, which a bit a word marks, so it costs what
/// the units see and a 64th of the map's words, not the map's size times the groups. A group has
/// a detection bitmap only once a unit of it detects.
#[derive(Debug, Default)]
pub(crate) struct SightMaps {
    /// The words of one bitmap.
    words: usize,
    /// Each group's bitmap, group after group.
    revealed: Vec<u64>,
    /// Each group's place among the detection bitmaps, once it has one.
    detection: Vec<Option<usize>>,
    detected: Vec<u64>,
    /// The words of `revealed`, then of `detected`, that this tick set, a bit a word, to clear
    /// the next.
    set_revealed: Vec<u64>,
    set_detected: Vec<u64>,
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
        self.set_revealed
            .resize(self.revealed.len().div_ceil(64), 0);
        self.set_detected.clear();
    }

    /// Clears what the tick before revealed and detected.
    pub(crate) fn begin_tick(&mut self) {
        SightMaps::clear_set(&mut self.revealed, &mut self.set_revealed);
        SightMaps::clear_set(&mut self.detected, &mut self.set_detected);
    }

    /// Clears each word of `words` that `set` marks, and the marks.
    fn clear_set(words: &mut [u64], set: &mut [u64]) {
        for (at, marks) in set.iter_mut().enumerate() {
            let mut left = mem::take(marks);
            while left != 0 {
                words[at * 64 + left.trailing_zeros() as usize] = 0;
                left &= left - 1;
            }
        }
    }

    /// Reveals `cells`, a run that is not empty, but those `hidden` holds, to `group`, and to its
    /// detection too when `detects`.
    pub(crate) fn reveal(
        &mut self,
        group: usize,
        cells: Range<usize>,
        detects: bool,
        hidden: Option<Hidden<'_>>,
    ) {
        let run = group * self.words;
        let bitmap = Bitmap {
            words: &mut self.revealed,
            set: &mut self.set_revealed,
            run,
        };
        bitmap.set(cells.clone(), hidden);
        if detects {
            let run = self.detection_slot(group) * self.words;
            let bitmap = Bitmap {
                words: &mut self.detected,
                set: &mut self.set_detected,
                run,
            };
            bitmap.set(cells, hidden);
        }
    }

    /// The place of `group`'s detection bitmap, which it gets on its first detector.
    fn detection_slot(&mut self, group: usize) -> usize {
        if let Some(slot) = self.detection[group] {
            return slot;
        }
        let slot = self.detected.len() / self.words.max(1);
        self.detected.resize(self.detected.len() + self.words, 0);
        self.set_detected
            .resize(self.detected.len().div_ceil(64), 0);
        self.detection[group] = Some(slot);
        slot
    }

    /// Whether `group` sees a cell of `cells`, a run: with its sight, or for a unit its tags
    /// hide, with its detection. A word at a time.
    pub(crate) fn sees(&self, group: usize, cells: Range<usize>, hidden: bool) -> bool {
        if cells.is_empty() {
            return false;
        }
        let (map, run) = if hidden {
            let Some(slot) = self.detection[group] else {
                return false;
            };
            (&self.detected, slot * self.words)
        } else {
            (&self.revealed, group * self.words)
        };
        let (first, last) = (cells.start / 64, (cells.end - 1) / 64);
        (first..=last).any(|word| {
            let mut bits = u64::MAX;
            if word == first {
                bits &= u64::MAX << (cells.start % 64);
            }
            if word == last {
                bits &= u64::MAX >> (63 - (cells.end - 1) % 64);
            }
            map[run + word] & bits != 0
        })
    }
}

/// One bitmap of the bitmaps `words`, at `run`, and the words of them a tick set, a bit a word.
#[derive(Debug)]
struct Bitmap<'a> {
    words: &'a mut [u64],
    set: &'a mut [u64],
    run: usize,
}

impl Bitmap<'_> {
    /// Sets the bits of `cells`, a run that is not empty, but those `hidden` holds, and notes each
    /// word it sets.
    fn set(self, cells: Range<usize>, hidden: Option<Hidden<'_>>) {
        debug_assert!(!cells.is_empty());
        let (first, last) = (cells.start / 64, (cells.end - 1) / 64);
        let from = u64::MAX << (cells.start % 64);
        let to = u64::MAX >> (63 - (cells.end - 1) % 64);
        for word in first..=last {
            let mut bits = u64::MAX;
            if word == first {
                bits &= from;
            }
            if word == last {
                bits &= to;
            }
            if let Some(hidden) = hidden {
                bits &= !hidden.word(word);
            }
            let at = self.run + word;
            self.words[at] |= bits;
            self.set[at / 64] |= 1 << (at % 64);
        }
    }
}

#[cfg(test)]
mod tests;
