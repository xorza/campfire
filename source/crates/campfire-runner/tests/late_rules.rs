//! The MOBA 3v3's late rules, which no short match reaches at level 1: an inhibitor's fall
//! and its respawn, the super creep of the next wave on its lane, the warden's blessing and its
//! respawn, and the core's fall, which ends the match. A hero deals each fatal blow through the
//! runner's world, as no input can at level 1, so the test pins rules, not a log: it has no
//! golden and no replay. The test pins the content's values, so a change to the content changes
//! it, by design.

use campfire_capabilities::internals::{self, queue_damage};
use campfire_capabilities::{MatchEnd, MatchResult, Stats, Team};
use campfire_common::Tick;
use campfire_math::Num;
use campfire_runner::internals::{MatchUnits, Moba3v3};

use crate::moba::{dead, gold, life, respawn_at, run_to, units_at};

#[test]
fn the_3v3s_late_rules_follow_each_fall() {
    // At 20 ticks a second the pick ends in tick 1199, which spawns the heroes and the camps and
    // sets the first wave for tick 2399 and the income for tick 1299 and every 100 ticks after.
    // Players 0 to 2 are north, 3 to 5 south; Kensho is player 3's hero, Husk player 2's.
    let moba = Moba3v3::load();
    let mut fixed = moba.start();
    run_to(&mut fixed, 1200);
    let world = fixed.runner().world();
    let units = MatchUnits::of_world(world);
    let kensho = units.hero(3);
    let husk = units.hero(2);
    let north_west_inhibitor = units.spawned_at(-11, -52);
    let warden = units.spawned_at(0, 0);
    let north_core = units.spawned_at(0, -54);

    // Kensho fells the north's west inhibitor in tick 1200: its 50 gold to player 3, and it
    // comes back 240 s, 4800 ticks, after the end of that tick, at the start of tick 6001.
    queue_damage(
        fixed.runner_mut().world_mut(),
        Some(kensho),
        north_west_inhibitor,
        Num::int(4000),
        "true",
    );
    run_to(&mut fixed, 1201);
    let world = fixed.runner().world();
    assert!(dead(world, north_west_inhibitor));
    assert_eq!(
        respawn_at(world, north_west_inhibitor),
        Some(Tick::new(6001))
    );
    assert_eq!(gold(world, &moba, 3), 50);

    // Husk fells the warden in tick 1201: its 150 gold to player 2; each north hero takes the
    // blessing, no south hero does; it comes back 360 s, 7200 ticks, after the end of that
    // tick, at tick 8402.
    queue_damage(
        fixed.runner_mut().world_mut(),
        Some(husk),
        warden,
        Num::int(5000),
        "true",
    );
    run_to(&mut fixed, 1202);
    let world = fixed.runner().world();
    assert!(dead(world, warden));
    assert_eq!(respawn_at(world, warden), Some(Tick::new(8402)));
    assert_eq!(gold(world, &moba, 2), 150);
    let blessing = Stats::modifier(world, 0, "warden_blessing").unwrap();
    let blessed: Vec<bool> = (0..Moba3v3::PLAYERS)
        .map(|slot| {
            internals::carried(world, MatchUnits::of_world(world).hero(slot))
                .contains(&(blessing, None))
        })
        .collect();
    assert_eq!(blessed, [true, true, true, false, false, false]);

    // The first wave, in tick 2399: south's west group, from its end at (−6, 50), walks against
    // the fallen inhibitor's lane and takes a super creep of 1500 life after its six; every
    // other group has its six. Income: 8 gold in each of ticks 1299 to 2399, 12 times.
    run_to(&mut fixed, 2400);
    let world = fixed.runner().world();
    let groups = [(-6, 50), (6, 50), (-6, -50), (6, -50)].map(|(x, z)| units_at(world, x, z));
    assert_eq!(groups.each_ref().map(Vec::len), [7, 6, 6, 6]);
    let supers: Vec<_> = groups
        .iter()
        .flatten()
        .filter(|&&unit| life(world, &moba, unit) == Num::int(1500))
        .collect();
    assert_eq!(supers, [&groups[0][6]]);
    assert_eq!(
        (gold(world, &moba, 2), gold(world, &moba, 3)),
        (96 + 150, 96 + 50)
    );

    // Kensho fells the north core in tick 2400: the match ends then, won by the south.
    queue_damage(
        fixed.runner_mut().world_mut(),
        Some(kensho),
        north_core,
        Num::int(5500),
        "true",
    );
    run_to(&mut fixed, 2401);
    let end = *fixed.runner().world().resource::<MatchEnd>();
    assert_eq!(
        (end.tick(), end.result()),
        (Tick::new(2400), MatchResult::Won(Team::new(1)))
    );
}
