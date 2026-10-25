use campfire_geometry::{Bounds, Grid};
use campfire_math::{Num, Vec3};

use super::*;

/// A sight's key in these tests: its place and range.
type Sighting = (Vec3, Num);

#[test]
fn a_slot_takes_its_runs_again_only_for_the_key_it_had_the_tick_before() {
    // 1 m cells over ±20 m; sights of 5 m and 6 m from two places a third of a meter apart.
    let bounds = Bounds::new([Num::int(-20); 2], [Num::int(20); 2]).unwrap();
    let grid = Grid::new(Num::ONE, bounds).unwrap();
    let at = |x: Num| Vec3::new(x, Num::ZERO, Num::int(2));
    let (here, there) = (at(Num::ONE), at(Num::ONE + Num::ONE / 3));
    let spans = |(position, range): Sighting| {
        let mut runs = Vec::new();
        grid.spans_within(position, range, |run| runs.push(run));
        runs
    };
    let mut cache = RunCache::default();
    // Each step: a tick of slots, each `(slot, key, from the grid)`, then the keys of the slots
    // the tick before took and this one did not.
    let (five, six) = (Num::int(5), Num::int(6));
    let ticks: [(&[(usize, Sighting, bool)], &[Sighting]); 7] = [
        (&[(0, (here, five), true), (3, (there, six), true)], &[]),
        // The same key takes the runs again; another place, or range, does not.
        (&[(0, (here, five), false), (3, (there, five), true)], &[]),
        (&[(0, (there, five), true), (3, (there, five), false)], &[]),
        // Slot 0 skips a tick: it is gone, and next takes its runs from the grid, with none
        // before.
        (&[(3, (there, five), false)], &[(there, five)]),
        (&[(0, (there, five), true), (3, (here, six), true)], &[]),
        // Another owner in slot 3 with its key takes its runs, which are that key's.
        (&[(0, (there, five), false), (3, (here, six), false)], &[]),
        (&[(0, (there, five), false)], &[(here, six)]),
    ];
    let mut before: Vec<Option<Sighting>> = vec![None; 4];
    for (tick, (slots, gone)) in ticks.into_iter().enumerate() {
        cache.begin_tick();
        let mut took = vec![None; 4];
        for &(slot, key, fresh) in slots {
            let mut asked = false;
            let runs = cache.runs(slot, key, |run| {
                asked = true;
                grid.spans_within(key.0, key.1, run);
            });
            assert_eq!(runs.now, spans(key), "tick {tick}, slot {slot}");
            assert_eq!(runs.key_before, before[slot], "tick {tick}, slot {slot}");
            assert_eq!(
                runs.before,
                before[slot].map_or_else(Vec::new, spans),
                "tick {tick}, slot {slot}"
            );
            assert_eq!(asked, fresh, "tick {tick}, slot {slot}");
            took[slot] = Some(key);
        }
        let mut ended = Vec::new();
        cache.end_tick(|key, runs| {
            assert_eq!(runs, spans(key), "tick {tick}");
            ended.push(key);
        });
        assert_eq!(ended, gone, "tick {tick}");
        before = took;
    }
    // A cleared cache, as for a new grid, finds each slot's runs on the grid again.
    cache.clear();
    cache.begin_tick();
    let mut asked = false;
    let runs = cache.runs(0, (there, five), |run| {
        asked = true;
        grid.spans_within(there, five, run);
    });
    assert!(asked && runs.key_before.is_none() && runs.before.is_empty());
}
