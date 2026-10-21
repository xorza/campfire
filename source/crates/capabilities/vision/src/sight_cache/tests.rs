use campfire_geometry::{Bounds, Grid};

use super::*;

#[test]
fn a_sight_takes_its_runs_again_only_from_the_same_place_and_range_of_the_tick_before() {
    // 1 m cells over ±20 m; sights of 5 m and 6 m from two places a third of a meter apart.
    let bounds = Bounds::new([Num::int(-20); 2], [Num::int(20); 2]).unwrap();
    let grid = Grid::new(Num::ONE, bounds).unwrap();
    let at = |x: Num| Vec3::new(x, Num::ZERO, Num::int(2));
    let (here, there) = (at(Num::ONE), at(Num::ONE + Num::ONE / 3));
    let spans = |position: Vec3, range: Num| {
        let mut runs = Vec::new();
        grid.spans_within(position, range, |run| runs.push(run));
        runs
    };
    let mut cache = SightCache::default();
    // Each step: a tick of sights, each `(slot, place, range, from the grid)`.
    let (five, six) = (Num::int(5), Num::int(6));
    let ticks: [&[(usize, Vec3, Num, bool)]; 7] = [
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
        for &(slot, position, range, fresh) in sights {
            let mut asked = false;
            let runs = cache
                .runs(slot, Sighting { position, range }, |run| {
                    asked = true;
                    grid.spans_within(position, range, run);
                })
                .to_vec();
            assert_eq!(runs, spans(position, range), "tick {tick}, slot {slot}");
            assert_eq!(asked, fresh, "tick {tick}, slot {slot}");
        }
    }
    // A cleared cache, as for a new grid, finds each sight's runs on the grid again.
    cache.clear();
    cache.begin_tick();
    let mut asked = false;
    let key = Sighting {
        position: there,
        range: five,
    };
    cache.runs(0, key, |run| {
        asked = true;
        grid.spans_within(there, five, run);
    });
    assert!(asked);
}
