use std::mem;
use std::ops::Range;

/// The runs of cells each slot took the tick before: runs that only their key decides, `K`, a
/// sight's look or a box's place and shape, so a slot whose key stood still takes them again
/// rather than the grid's rows, and one whose key changed has its runs before beside its runs now.
/// A slot is any number its caller keeps for it: another owner in a slot takes runs only from the
/// key they belong to. Both ticks' runs lie in two buffers, one run after another, as a buffer for
/// each slot would cost an allocation each.
#[derive(Debug)]
pub(crate) struct RunCache<K> {
    /// Each slot's runs, as a tick last took them.
    slots: Vec<Option<Taken<K>>>,
    /// The slots the running tick took, and those the tick before took.
    took: Vec<usize>,
    took_before: Vec<usize>,
    /// The runs of the running tick's slots, and of the tick before's.
    runs: Vec<Range<usize>>,
    kept: Vec<Range<usize>>,
    tick: u64,
}

/// A slot's runs: their key, the tick they were taken in, and where they lie among that tick's.
#[derive(Debug, Clone, Copy)]
struct Taken<K> {
    key: K,
    tick: u64,
    first: u32,
    end: u32,
}

/// What a slot took: its runs now, and its key and runs the tick before, if it took any then,
/// none else.
#[derive(Debug)]
pub(crate) struct Runs<'a, K> {
    pub(crate) now: &'a [Range<usize>],
    pub(crate) key_before: Option<K>,
    pub(crate) before: &'a [Range<usize>],
}

impl<K> Default for RunCache<K> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            took: Vec::new(),
            took_before: Vec::new(),
            runs: Vec::new(),
            kept: Vec::new(),
            tick: 0,
        }
    }
}

impl<K: Copy + PartialEq> RunCache<K> {
    /// Forgets every slot, as on a new grid.
    pub(crate) fn clear(&mut self) {
        self.slots.clear();
        self.took.clear();
        self.took_before.clear();
        self.runs.clear();
        self.kept.clear();
    }

    /// Starts a tick: the running tick's runs become those it takes again from.
    pub(crate) fn begin_tick(&mut self) {
        mem::swap(&mut self.runs, &mut self.kept);
        mem::swap(&mut self.took, &mut self.took_before);
        self.runs.clear();
        self.took.clear();
        self.tick += 1;
    }

    /// The runs of `slot` for `key`: the tick before's, when it took them for the same key, else
    /// those `spans` gives; with its key and runs the tick before.
    pub(crate) fn runs(
        &mut self,
        slot: usize,
        key: K,
        spans: impl FnOnce(&mut dyn FnMut(Range<usize>)),
    ) -> Runs<'_, K> {
        if self.slots.len() <= slot {
            self.slots.resize_with(slot + 1, || None);
        }
        debug_assert!(
            self.slots[slot].is_none_or(|taken| taken.tick != self.tick),
            "a slot takes its runs once a tick"
        );
        let before = self.slots[slot].filter(|taken| taken.tick + 1 == self.tick);
        let first = self.runs.len();
        match before {
            Some(taken) if taken.key == key => {
                self.runs.extend_from_slice(Self::of(&self.kept, taken));
            }
            _ => spans(&mut |run| self.runs.push(run)),
        }
        let count = |at: usize| u32::try_from(at).expect("a tick's runs fit u32");
        self.slots[slot] = Some(Taken {
            key,
            tick: self.tick,
            first: count(first),
            end: count(self.runs.len()),
        });
        self.took.push(slot);
        Runs {
            now: &self.runs[first..],
            key_before: before.map(|taken| taken.key),
            before: before.map_or(&[], |taken| Self::of(&self.kept, taken)),
        }
    }

    /// Calls `gone` with the key and runs of each slot the tick before took and this tick did
    /// not, and forgets it.
    pub(crate) fn end_tick(&mut self, mut gone: impl FnMut(K, &[Range<usize>])) {
        for &slot in &self.took_before {
            let taken = self.slots[slot].expect("a slot the tick before took");
            if taken.tick != self.tick {
                gone(taken.key, Self::of(&self.kept, taken));
                self.slots[slot] = None;
            }
        }
    }

    /// The runs of `taken` among its tick's `runs`.
    fn of(runs: &[Range<usize>], taken: Taken<K>) -> &[Range<usize>] {
        &runs[taken.first as usize..taken.end as usize]
    }
}

#[cfg(test)]
mod tests;
