//! The MOBA 3v3's late rules, which no short match reaches at level 1: an inhibitor's fall and its
//! respawn, the super creep of the next wave on its lane, the warden's blessing and its respawn, a
//! hero's kill that a tower assisted, and the core's fall, which ends the match, a draw when both
//! cores fall in one tick. A hero deals each fatal blow through the runner's world, as no input can
//! at level 1, so the tests pin rules, not a log: it has no golden and no replay. They pin the
//! content's values, so a change to the content changes it, by design.

use campfire_combat::{Combat, Respawn};
use campfire_common::Tick;
use campfire_harness::{MatchUnits, Moba3v3};
use campfire_math::Num;
use campfire_mode::{MatchResult, ModeWorld};
use campfire_sim::internals::SimWorldInternals;
use campfire_stats::{Life, Stats};
use campfire_units::Dead;
use campfire_values::{PackageIndex, Team};

use crate::runner::moba::{gold, run_to};

#[test]
fn the_3v3s_late_rules_follow_each_fall() {
    // At 20 ticks a second the pick ends in tick 1199, which spawns the heroes and the camps and
    // sets the first wave for tick 2399 and the income for tick 1299 and every 100 ticks after.
    // Players 0 to 2 are north, 3 to 5 south; Kensho is player 3's hero, Husk player 2's.
    let moba = Moba3v3::load();
    let mut fixed = moba.start();
    run_to(&mut fixed, 1200);
    let world = fixed.game();
    let units = MatchUnits::of(world);
    let kensho = units.hero(3);
    let husk = units.hero(2);
    let north_west_inhibitor = units.spawned_at((-11, 0, -52));
    let warden = units.spawned_at((0, 0, 0));
    let north_core = units.spawned_at((0, 0, -54));
    let south_west_tower = units.spawned_at((-22, 0, 46));
    let assister = units.hero(4);

    // Kensho fells the north's west inhibitor in tick 1200: its 50 gold to player 3, and it
    // comes back 240 s, 4800 ticks, after the end of that tick, at the start of tick 6001.
    Combat::queue_damage(
        fixed.game_mut(),
        Some(kensho),
        north_west_inhibitor,
        Num::int(4000),
        "true",
    );
    run_to(&mut fixed, 1201);
    let world = fixed.game();
    assert!(world.try_part::<Dead>(north_west_inhibitor).is_some());
    assert_eq!(
        world
            .try_part::<Respawn>(north_west_inhibitor)
            .map(|respawn| respawn.at),
        Some(Tick::new(6001))
    );
    assert_eq!(gold(&moba, world, 3), 50);

    // Husk fells the warden in tick 1201: its 150 gold to player 2; each north hero takes the
    // blessing, no south hero does; it comes back 360 s, 7200 ticks, after the end of that
    // tick, at tick 8402.
    Combat::queue_damage(fixed.game_mut(), Some(husk), warden, Num::int(5000), "true");
    run_to(&mut fixed, 1202);
    let world = fixed.game();
    assert!(world.try_part::<Dead>(warden).is_some());
    assert_eq!(
        world.try_part::<Respawn>(warden).map(|respawn| respawn.at),
        Some(Tick::new(8402))
    );
    assert_eq!(gold(&moba, world, 2), 150);
    let blessing = Stats::modifier(world, PackageIndex::MODE, "warden_blessing").unwrap();
    let blessed: Vec<bool> = (0..Moba3v3::PLAYERS)
        .map(|slot| {
            Stats::carried_modifiers(world, MatchUnits::of(world).hero(slot))
                .contains(&(blessing, None))
        })
        .collect();
    assert_eq!(blessed, [true, true, true, false, false, false]);

    // The first wave, in tick 2399: south's west group, from its end at (−6, 50), walks against
    // the fallen inhibitor's lane and takes a super creep of 1500 life after its six; every
    // other group has its six. Income: 8 gold in each of ticks 1299 to 2399, 12 times.
    run_to(&mut fixed, 2400);
    let world = fixed.game();
    let groups = [(-6, 50), (6, 50), (-6, -50), (6, -50)]
        .map(|(x, z)| MatchUnits::of(world).standing_at((x, 0, z)));
    assert_eq!(groups.each_ref().map(Vec::len), [7, 6, 6, 6]);
    let supers: Vec<_> = groups
        .iter()
        .flatten()
        .filter(|&&unit| world.part::<Life>(unit).current() == Num::int(1500))
        .collect();
    assert_eq!(supers, [&groups[0][6]]);
    assert_eq!(
        (gold(&moba, world, 2), gold(&moba, world, 3)),
        (96 + 150, 96 + 50)
    );

    // In tick 2400 the south's west tower at (−22, 46) and player 4's hero strike Husk; in tick
    // 2401 Kensho kills him, as the pass deals a tick's damage by its source's stable id, which
    // would deal Kensho's first. The match's first hero kill gives player 3 the kill gold, 300,
    // and the first blood, 100; the tower takes no share of the assist gold, so player 4's hero,
    // the one hero assister, takes all 150. No income falls in either tick.
    for source in [south_west_tower, assister] {
        Combat::queue_damage(fixed.game_mut(), Some(source), husk, Num::ONE, "true");
    }
    run_to(&mut fixed, 2401);
    Combat::queue_damage(fixed.game_mut(), Some(kensho), husk, Num::int(5000), "true");
    run_to(&mut fixed, 2402);
    let world = fixed.game();
    assert!(world.try_part::<Dead>(husk).is_some());
    assert_eq!(
        [2, 3, 4].map(|slot| gold(&moba, world, slot)),
        [96 + 150, 96 + 50 + 300 + 100, 96 + 150]
    );

    // Kensho fells the north core in tick 2402: the match ends then, won by the south.
    Combat::queue_damage(
        fixed.game_mut(),
        Some(kensho),
        north_core,
        Num::int(5500),
        "true",
    );
    run_to(&mut fixed, 2403);
    let end = fixed.game().match_end().expect("the match ended");
    assert_eq!(
        (end.tick(), end.result()),
        (Tick::new(2402), MatchResult::Won(Team::new(1)))
    );
}

#[test]
fn both_cores_falling_in_one_tick_end_the_3v3_in_a_draw() {
    // Each core takes its whole life in tick 1: each death is heard after both, so the first
    // heard finds no core standing, ends the match a draw, and the second ends nothing more.
    let moba = Moba3v3::load();
    let mut fixed = moba.start();
    run_to(&mut fixed, 1);
    let units = MatchUnits::of(fixed.game());
    let cores = [(0, 0, -54), (0, 0, 54)].map(|at| units.spawned_at(at));
    for core in cores {
        Combat::queue_damage(fixed.game_mut(), None, core, Num::int(5500), "true");
    }
    run_to(&mut fixed, 2);
    let end = fixed.game().match_end().expect("the match ended");
    assert_eq!(
        (end.tick(), end.result()),
        (Tick::new(1), MatchResult::Draw)
    );
}
