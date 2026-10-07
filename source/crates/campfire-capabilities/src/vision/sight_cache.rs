use std::mem;
use std::ops::Range;

use campfire_math::Num;
use campfire_sim::Position;

/// The runs of cells each seer's sight covered the tick before. A sight covers the same runs
/// from the same place with the same range on the same grid, so a seer that stood still takes
/// them again rather than the grid's rows. A seer is a slot, any number its caller keeps for it:
/// another seer in a slot takes runs only from the place and the range they belong to.
#[derive(Debug, Default)]
pub(crate) struct SightCache {
    /// Each slot's sight, as a tick last took it.
    seers: Vec<Option<CachedSight>>,
    /// The runs of the running tick's sights, and of the tick before's.
    runs: Vec<Range<usize>>,
    kept: Vec<Range<usize>>,
    tick: u64,
}

/// A slot's sight: where from, how far, in which tick, and its runs among that tick's.
#[derive(Debug, Clone, Copy)]
struct CachedSight {
    pos: Position,
    range: Num,
    tick: u64,
    first: u32,
    end: u32,
}

impl SightCache {
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

    /// The runs of the sight of `slot` from `pos` within `range`: the tick before's, when it saw
    /// from there as far, else those `spans` gives.
    pub(crate) fn runs(
        &mut self,
        slot: usize,
        pos: Position,
        range: Num,
        spans: impl FnOnce(&mut dyn FnMut(Range<usize>)),
    ) -> &[Range<usize>] {
        let first = self.runs.len();
        let before = self.seers.get(slot).copied().flatten();
        match before {
            Some(seen) if seen.tick + 1 == self.tick && seen.pos == pos && seen.range == range => {
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
            pos,
            range,
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
    use crate::values::bounds::Bounds;
    use crate::values::grid::Grid;

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
                    .runs(slot, pos, range, |run| {
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
        cache.runs(0, there, five, |run| {
            asked = true;
            grid.spans_within(there, five, run);
        });
        assert!(asked);
    }
}
