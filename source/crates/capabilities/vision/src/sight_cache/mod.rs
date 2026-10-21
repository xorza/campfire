use std::mem;
use std::ops::Range;

use campfire_math::{Num, Vec3};

/// The runs of cells each seer's sight covered the tick before, or each box: runs that only their
/// key decides, `K`, a sight's place and range or a box's place and shape, so a seer that stood
/// still takes them again rather than the grid's rows. A seer is a slot, any number its caller
/// keeps for it: another seer in a slot takes runs only from the key they belong to.
#[derive(Debug)]
pub(crate) struct SightCache<K> {
    /// Each slot's runs, as a tick last took them.
    seers: Vec<Option<CachedSight<K>>>,
    /// The runs of the running tick's sights, and of the tick before's.
    runs: Vec<Range<usize>>,
    kept: Vec<Range<usize>>,
    tick: u64,
}

/// A sight's key: where from, and how far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sighting {
    pub position: Vec3,
    pub(crate) range: Num,
}

/// A slot's runs: their key, the tick they were taken in, and where they lie among that tick's.
#[derive(Debug, Clone, Copy)]
struct CachedSight<K> {
    key: K,
    tick: u64,
    first: u32,
    end: u32,
}

impl<K> Default for SightCache<K> {
    fn default() -> Self {
        Self {
            seers: Vec::new(),
            runs: Vec::new(),
            kept: Vec::new(),
            tick: 0,
        }
    }
}

impl<K: Copy + PartialEq> SightCache<K> {
    /// Forgets every sight, as on a new grid.
    pub(crate) fn clear(&mut self) {
        self.seers.clear();
        self.runs.clear();
        self.kept.clear();
    }

    /// Starts a tick: the running tick's runs become those it takes again from.
    pub(crate) fn begin_tick(&mut self) {
        mem::swap(&mut self.runs, &mut self.kept);
        self.runs.clear();
        self.tick += 1;
    }

    /// The runs of `slot` for `key`: the tick before's, when it took them for the same key, else
    /// those `spans` gives.
    pub(crate) fn runs(
        &mut self,
        slot: usize,
        key: K,
        spans: impl FnOnce(&mut dyn FnMut(Range<usize>)),
    ) -> &[Range<usize>] {
        let first = self.runs.len();
        let before = self.seers.get(slot).copied().flatten();
        match before {
            Some(seen) if seen.tick + 1 == self.tick && seen.key == key => {
                let kept = &self.kept[seen.first as usize..seen.end as usize];
                self.runs.extend_from_slice(kept);
            }
            _ => spans(&mut |run| self.runs.push(run)),
        }
        if self.seers.len() <= slot {
            self.seers.resize(slot + 1, None);
        }
        let count = |at: usize| u32::try_from(at).expect("a tick's runs fit u32");
        self.seers[slot] = Some(CachedSight {
            key,
            tick: self.tick,
            first: count(first),
            end: count(self.runs.len()),
        });
        &self.runs[first..]
    }
}

#[cfg(test)]
mod tests;
