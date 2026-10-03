//! The reference 3v3's west camp, played by the north's scripted players: its wolf answers the
//! hero that strikes it, goes home past its leash, and falls to three heroes, its bounty, its
//! experience and its respawn as the mode's script gives them. The test pins the content's
//! values, so a change to the content changes it, by design.

use campfire_capabilities::{ActionSlots, Deaths, ScriptFailures};
use campfire_common::Tick;
use campfire_math::{Num, Vec3};
use campfire_runner::internals::{Golden, HashTrail, MatchUnits, Play, Reference3v3};
use campfire_sim::{EntityIndex, Position, StableId};

use crate::match_3v3::assert_replays;
use crate::reference::{gold, level_xp, respawn_at};

/// The ticks the camp plays: the pick, the walk, the leash and the fall.
const TICKS: u64 = 2000;

#[test]
fn the_west_camps_wolf_answers_resets_and_falls_to_the_north() {
    // Players 0 to 2 hold Cinder, Gale and Husk, north. At 20 ticks a second the heroes spawn at
    // the pick's end, in tick 1199. Husk strikes the wolf, then walks south from tick 1560; the
    // three strike it from tick 1700, and Husk fells it.
    let reference = Reference3v3::load();
    let mut fixed = reference.start();
    let mut trail = HashTrail::default();
    let mut golden = Golden::new(reference.packages(), Reference3v3::PLAYERS);
    let home = Position::new(Vec3::new(Num::int(-18), Num::ZERO, Num::int(-12))).unwrap();
    let mut deaths = Vec::new();
    let mut farthest = Num::ZERO;
    let mut seen = Vec::new();
    for tick in 0..TICKS {
        Reference3v3::play_tick(&mut fixed, Play::Camp, tick);
        let runner = fixed.runner();
        trail.record(runner.world());
        golden.record(runner);
        let world = runner.world();
        let failures = world.non_send::<ScriptFailures>();
        assert!(
            failures.get().is_empty(),
            "tick {tick}: {:?}",
            failures.get()
        );
        deaths.extend(world.resource::<Deaths>().iter().map(|death| {
            let assisters = death.assisters.to_vec();
            (tick, death.fallen.unit, death.killer, assisters)
        }));
        if tick < 1199 {
            continue;
        }
        let units = MatchUnits::of_world(world);
        let wolf = units.spawned_at(-18, -12);
        let at = units.position(wolf);
        if (1560..1680).contains(&tick) {
            farthest = farthest.max(at.get().distance(home.get()));
        }
        if [1540, 1680].contains(&tick) {
            let entity = world.resource::<EntityIndex>().get(wolf).unwrap();
            let target = world.get::<ActionSlots>(entity).unwrap().attack_target();
            seen.push((target, at));
        }
    }
    golden.check("3v3-camp");
    let world = fixed.runner().world();
    let units = MatchUnits::of_world(world);
    let [cinder, gale, husk] = [0, 1, 2].map(|slot| units.hero(slot));
    let wolf = units.spawned_at(-18, -12);

    // In tick 1540 the wolf answers Husk, who struck it; it chases him south past its 8 m leash,
    // and in tick 1680 it stands on its camp again, with no target.
    assert_eq!(seen[0].0, Some(husk));
    assert!(farthest > Num::int(8), "{farthest}");
    assert_eq!(seen[1], (None, home));

    // Husk fells it in tick 1956, Cinder and Gale assisting by stable id: its 30 gold to player
    // 2 alone, and its 90 experience split among the three heroes within 16 m, 30 each. It comes
    // back 60 s, 1200 ticks, after the end of that tick: in tick 3157.
    let fell: Vec<(u64, StableId, Option<StableId>, Vec<StableId>)> = deaths;
    assert_eq!(fell, [(1956, wolf, Some(husk), vec![cinder, gale])]);
    let gold = [0, 1, 2].map(|slot| gold(world, &reference, slot));
    assert_eq!([gold[1] - gold[0], gold[2] - gold[0]], [0, 30]);
    let xp = [cinder, gale, husk].map(|unit| level_xp(world, &reference, unit));
    assert_eq!(xp, [Num::int(30); 3]);
    assert_eq!(respawn_at(world, wolf), Some(Tick::new(3157)));
    assert_replays(&reference, fixed.runner(), &trail);
}
