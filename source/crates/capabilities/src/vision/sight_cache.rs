use std::mem;
use std::ops::Range;

use campfire_math::Num;
use campfire_sim::Position;

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
    pub(crate) pos: Position,
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
    fn default() -> SightCache<K> {
        SightCache {
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
mod tests {
    use campfire_math::Vec3;

    use super::*;
    use crate::geometry::bounds::Bounds;
    use crate::geometry::grid::Grid;

    #[test]
    fn a_sight_takes_its_runs_again_only_from_the_same_place_and_range_of_the_tick_before() {
        // 1 m cells over ±20 m; sights of 5 m and 6 m from two places a third of a meter apart.
        let bounds = Bounds::new([Num::int(-20); 2], [Num::int(20); 2]).unwrap();
        let grid = Grid::new(Num::ONE, bounds).unwrap();
        let at = |x: Num| Position::new(Vec3::new(x, Num::ZERO, Num::int(2))).unwrap();
        let (here, there) = (at(Num::ONE), at(Num::ONE + Num::ONE / 3));
        let spans = |pos: Position, range: Num| {
            let mut runs = Vec::new();
            grid.spans_within(pos, range, |run| runs.push(run));
            runs
        };
        let mut cache = SightCache::default();
        // Each step: a tick of sights, each `(slot, place, range, from the grid)`.
        let (five, six) = (Num::int(5), Num::int(6));
        let ticks: [&[(usize, Position, Num, bool)]; 7] = [
            &[(0, here, five, true), (3, there, six, true)],
            // The same place and range take the runs again; another place, or range, does not.
            &[(0, here, five, false), (3, there, five, true)],
            &[(0, there, five, true), (3, there, five, false)],
            // Slot 0 skips a tick: its sight is no longer the tick before's.
            &[(3, there, five, false)],
            &[(0, there, five, true), (3, here, six, true)],
            // Another seer in slot 3 at its place takes its runs, which are that place's.
            &[(0, there, five, false), (3, here, six, false)],
            &[(0, there, five, false)],
        ];
        for (tick, sights) in ticks.into_iter().enumerate() {
            cache.begin_tick();
            for &(slot, pos, range, fresh) in sights {
                let mut asked = false;
                let runs = cache
                    .runs(slot, Sighting { pos, range }, |run| {
                        asked = true;
                        grid.spans_within(pos, range, run);
                    })
                    .to_vec();
                assert_eq!(runs, spans(pos, range), "tick {tick}, slot {slot}");
                assert_eq!(asked, fresh, "tick {tick}, slot {slot}");
            }
        }
        // A cleared cache, as for a new grid, finds each sight's runs on the grid again.
        cache.clear();
        cache.begin_tick();
        let mut asked = false;
        let key = Sighting {
            pos: there,
            range: five,
        };
        cache.runs(0, key, |run| {
            asked = true;
            grid.spans_within(there, five, run);
        });
        assert!(asked);
    }
}
