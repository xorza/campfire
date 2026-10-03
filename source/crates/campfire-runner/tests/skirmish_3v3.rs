//! The reference 3v3 before its first wave, played by scripted players: first blood and its
//! assist, the experience a hero's death shares, mend and haste, a tower that turns on the hero
//! who strikes an allied hero, and the respawns of fallen heroes, as the mode's script gives
//! them. The test pins the content's values, so a change to the content changes it, by design.

use campfire_capabilities::internals;
use campfire_capabilities::{ActionSlots, Deaths, ScriptFailures, Stats};
use campfire_common::Tick;
use campfire_math::Num;
use campfire_runner::internals::{Golden, HashTrail, MatchUnits, Play, Reference3v3};
use campfire_sim::{EntityIndex, StableId};

use crate::match_3v3::assert_replays;
use crate::reference::{gold, level_xp, life, respawn_at};

/// The ticks the skirmish plays: the pick, the fight in the middle, and the dive.
const TICKS: u64 = 2320;

#[test]
fn a_skirmish_pays_first_blood_and_a_tower_turns_on_the_diver() {
    // Players 0 to 5 hold Cinder, Gale, Husk, north, and Kensho, Rime, Veil, south. At 20 ticks
    // a second the heroes spawn in tick 1199. Husk casts haste then; Cinder and Gale strike Rime
    // in the middle from tick 1700, and she strikes Gale; Gale casts mend in tick 1880. Cinder
    // walks into the range of the south's west outer tower, Husk strikes Kensho beside it, and
    // falls to the tower.
    let reference = Reference3v3::load();
    let mut fixed = reference.start();
    let mut trail = HashTrail::default();
    let mut golden = Golden::new(reference.packages(), Reference3v3::PLAYERS);
    let spells = reference
        .packages()
        .packages()
        .position(|view| view.package.header.name == "player-spells")
        .unwrap();
    let spells = u16::try_from(spells).unwrap();
    let mut deaths = Vec::new();
    let mut gale_lives = Vec::new();
    let mut hasted = Vec::new();
    let mut kensho_lives = Vec::new();
    let mut tower_targets = Vec::new();
    for tick in 0..TICKS {
        Reference3v3::play_tick(&mut fixed, Play::Skirmish, tick);
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
        for death in world.resource::<Deaths>().iter() {
            let unit = death.fallen.unit;
            let assisters = death.assisters.to_vec();
            deaths.push((tick, unit, death.killer, assisters, respawn_at(world, unit)));
        }
        if tick < 1199 {
            continue;
        }
        let units = MatchUnits::of_world(world);
        let [gale, husk, kensho] = [1, 2, 3].map(|slot| units.hero(slot));
        if (1878..=1880).contains(&tick) {
            gale_lives.push(life(world, &reference, gale));
        }
        if [1400, 1401].contains(&tick) {
            let haste = Stats::modifier(world, spells, "haste").unwrap();
            let carried = internals::carried(world, husk);
            hasted.push(carried.iter().any(|&(id, _)| id == haste));
        }
        if (2190..2220).contains(&tick) {
            let tower = units.spawned_at(-42, 16);
            let entity = world.resource::<EntityIndex>().get(tower).unwrap();
            tower_targets.push(world.get::<ActionSlots>(entity).unwrap().attack_target());
            kensho_lives.push(life(world, &reference, kensho));
        }
    }
    golden.check("3v3-skirmish");
    let world = fixed.runner().world();
    let units = MatchUnits::of_world(world);
    let heroes: [StableId; 6] = [0, 1, 2, 3, 4, 5].map(|slot| units.hero(slot));
    let [cinder, gale, husk, _, rime, _] = heroes;
    let tower = units.spawned_at(-42, 16);

    // Cinder fells Rime in tick 1840, Gale assisting, and the tower fells Husk in tick 2311,
    // with no hero assisting. A hero of level 1 comes back 5000 + 2500 × 1 ms, 150 ticks, after
    // the end of the tick it fell in.
    assert_eq!(
        deaths,
        [
            (
                1840,
                rime,
                Some(cinder),
                vec![gale],
                Some(Tick::new(1840 + 1 + 150))
            ),
            (
                2311,
                husk,
                Some(tower),
                vec![],
                Some(Tick::new(2311 + 1 + 150))
            ),
        ]
    );

    // First blood: Cinder's player takes 300 + 100 gold, and Gale's the 150 of the assist,
    // split among one assister. A tower's kill pays no one. Every player has the same income.
    let base = gold(world, &reference, 2);
    let gold: Vec<i64> = (0..6)
        .map(|slot| gold(world, &reference, slot) - base)
        .collect();
    assert_eq!(gold, [400, 150, 0, 0, 0, 0]);

    // A fallen hero of level 1 shares 150 + 25 × 1 experience among the enemy heroes within
    // 16 m: Rime's between Cinder and Gale, 87.5 each; Husk's to Kensho, beside the tower.
    let xp = heroes.map(|unit| level_xp(world, &reference, unit));
    let half = Num::int(175) / 2;
    assert_eq!(
        xp,
        [half, half, Num::ZERO, Num::int(175), Num::ZERO, Num::ZERO]
    );

    // Mend heals Gale by 75 + 15 × (1 − 1) in tick 1880, beside the regen of every tick.
    let regen = gale_lives[1] - gale_lives[0];
    assert_eq!(gale_lives[2] - gale_lives[1] - regen, Num::int(75));

    // Haste, cast in tick 1200, lasts 10 s, 200 ticks: Husk holds it after tick 1400, and its
    // end comes as tick 1401 starts.
    assert_eq!(hasted, [true, false]);

    // The tower holds Cinder, the nearest hero in its range, until Husk strikes Kensho, which
    // turns it on Husk within one think of 250 ms, 5 ticks.
    let struck = kensho_lives
        .iter()
        .position(|&life| life < Num::int(444))
        .unwrap();
    assert_eq!(tower_targets[struck - 1], Some(cinder));
    assert_eq!(tower_targets[struck + 5], Some(husk));
    assert_replays(&reference, fixed.runner(), &trail);
}
