//! The test lane mode plays 30 s to its golden record, whatever its heroes' text says: its walker
//! learns with its points, and fells creeps for their experience.

use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_capabilities::{
    Action, ActionSlots, Deaths, Experience, Hook, Level, Order, PathWalker, Points, Rank,
    ScriptFailures, Team, TrackId,
};
use campfire_common::{MapName, StateHash, Tick};
use campfire_math::Num;
use campfire_package::{ModePackages, PackageDir};
use campfire_runner::ScriptCallFailed;
use campfire_runner::internals::{CopyCheck, FixedMatch, FixedSession, Golden, MatchUnits};
use campfire_sim::{EntityIndex, StableId};

/// The walker's learning and progress as a tick left them: the ranks of its basic abilities and
/// its ultimate, its points, its experience and its level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Progress {
    ranks: [u8; 4],
    points: u32,
    xp: Num,
    level: u32,
}

impl Progress {
    fn of(fixed: &FixedMatch, hero: StableId) -> Progress {
        let world = fixed.runner().world();
        let unit = world.entity(world.resource::<EntityIndex>().get(hero).unwrap());
        let slots = unit.get::<ActionSlots>().unwrap();
        let track = TrackId::new(0);
        Progress {
            ranks: [0, 1, 2, 3].map(|slot| Rank::count(slots.slot(slot).unwrap().rank)),
            points: unit.get::<Points>().unwrap().get(),
            xp: unit.get::<Experience>().unwrap().get(track).unwrap().xp,
            level: unit.get::<Level>().unwrap().get(),
        }
    }
}

/// Player 0 sends `action` for the walker, stamped for `tick`.
fn order(fixed: &mut FixedMatch, walker: StableId, tick: u64, action: Action) {
    let payload = Order::payload(&[Order::one(walker, action)]);
    fixed.send(0, Tick::new(tick), &payload);
}

/// The ticks the lane match plays, and the one its east creeps reach the walker in.
const TICKS: u64 = 900;
const MEET: u64 = 140;

/// The mode's scripted match, which `source/.config/nextest.toml` exempts from the bound of 1 s.
mod scripted_match {
    use super::*;

    #[test]
    fn the_lane_match_plays_to_its_golden_record() {
        let packages = ModePackages::from_dir(
            &PackageDir::workspace("test/modes/lane"),
            &MapName::new("lane").unwrap(),
        )
        .unwrap();
        let session = FixedSession::new(packages, NonZeroU32::new(30).unwrap(), 2);
        let mut golden = Golden::new(session.packages(), session.slots());
        let mut fixed = session.start();
        let mut copy = CopyCheck::new(fixed.runner_mut());
        let walker = MatchUnits::of(&fixed).hero(0);
        // The first wave's creeps of the east, which spawn in tick 0, walk to the walker and strike
        // it as they meet; the west's strike the runner, so the walker alone fells these.
        let mut east = Vec::new();
        let mut felled = Vec::new();
        let mut seen = Vec::new();
        for tick in 0..TICKS {
            // Tick 1: the walker learns its first ability with its spawn point. As they meet, it
            // attacks the first east creep, and the second once the first falls. Once both fell, it
            // tries its ultimate, then a second rank of its first ability.
            let next = felled.len();
            match tick {
                1 => order(&mut fixed, walker, tick, Action::Learn { slot: 0 }),
                MEET => order(&mut fixed, walker, tick, Action::Attack { target: east[0] }),
                _ if next == 1 && felled[0] + 1 == tick => {
                    order(&mut fixed, walker, tick, Action::Attack { target: east[1] });
                }
                _ if next == 2 && felled[1] + 1 == tick => {
                    order(&mut fixed, walker, tick, Action::Learn { slot: 3 });
                    order(&mut fixed, walker, tick, Action::Learn { slot: 0 });
                }
                _ => {}
            }
            fixed.runner_mut().run_tick();
            golden.record(fixed.runner());
            copy.check(fixed.runner_mut());
            if tick == 0 {
                east = MatchUnits::of(&fixed)
                    .all()
                    .filter(|(_, unit)| {
                        unit.contains::<PathWalker>() && unit.get::<Team>() == Some(&Team::new(1))
                    })
                    .map(|(id, _)| id)
                    .collect();
            }
            let world = fixed.runner().world();
            let failures = world.non_send::<ScriptFailures>();
            assert!(
                failures.get().is_empty(),
                "tick {tick}: {:?}",
                failures.get()
            );
            for death in world.resource::<Deaths>().iter() {
                if east.contains(&death.fallen.unit) {
                    assert_eq!(death.killer, Some(walker), "tick {tick}");
                    felled.push(tick);
                }
            }
            if tick == 1
                || felled.last() == Some(&tick)
                || felled.get(1).map(|&at| at + 1) == Some(tick)
            {
                seen.push(Progress::of(&fixed, walker));
            }
        }
        golden.check("lane");
        // After tick 1: its first ability at rank 1, for its one point. Each creep it fells gives it
        // 60 experience: 120 after the second, past level 2's 100, which gives a point. Its ultimate
        // needs level 3, and its first ability's rank 2 level 2: the point goes to that.
        let progress = |ranks, points, xp, level| Progress {
            ranks,
            points,
            xp: Num::int(xp),
            level,
        };
        assert_eq!(
            seen,
            [
                progress([1, 0, 0, 0], 0, 0, 1),
                progress([1, 0, 0, 0], 0, 60, 1),
                progress([1, 0, 0, 0], 1, 120, 2),
                progress([2, 0, 0, 0], 0, 120, 2),
            ]
        );
    }
}

#[test]
fn the_lane_matchs_hashes_do_not_change_with_its_heroes_text() {
    // Walker's name in other words, and in a second language: its package's fingerprint moves,
    // and no tick's hash does, as the sim reads no text.
    let mut files = PackageDir::workspace_tree("test");
    let plain = PackageDir::in_memory(Arc::new(files.clone()), "modes/lane");
    files.insert(
        "heroes/walker/locale/en.ftl".into(),
        b"hero-name = Wanderer\n".to_vec(),
    );
    files.insert(
        "heroes/walker/locale/de.ftl".into(),
        b"hero-name = Wanderer\n".to_vec(),
    );
    let reworded = PackageDir::in_memory(Arc::new(PackageDir::reindexed(files)), "modes/lane");
    let [plain, reworded] = [plain, reworded]
        .map(|dir| ModePackages::from_package_dir(&dir, &MapName::new("lane").unwrap()).unwrap());
    let walker = |packages: &ModePackages| {
        let walker = packages
            .packages()
            .find(|view| view.package.header.name == "hero-walker");
        walker.unwrap().package.fingerprint
    };
    assert_ne!(walker(&plain), walker(&reworded));
    assert_eq!(hashes(plain), hashes(reworded));
}

#[test]
fn a_failed_script_call_logs_its_tick_unit_hook_and_why() {
    let mut files = PackageDir::workspace_tree("test");
    let script = "modes/lane/scripts/tower_ai.rhai".into();
    let text = String::from_utf8(files[&script].clone()).unwrap();
    let think = "fn on_think(ctx, tower) {\n";
    assert!(text.contains(think));
    let text = text.replacen(
        think,
        "fn on_think(ctx, tower) {\n    throw \"no think\";\n",
        1,
    );
    files.insert(script, text.into_bytes());
    let dir = PackageDir::in_memory(Arc::new(PackageDir::reindexed(files)), "modes/lane");
    let session = FixedSession::new(
        ModePackages::from_package_dir(&dir, &MapName::new("lane").unwrap()).unwrap(),
        NonZeroU32::new(30).unwrap(),
        2,
    );
    let mut fixed = session.start();
    for _ in 0..10 {
        fixed.runner_mut().run_tick();
    }
    // The towers, the map's first units, think every ⌈250 ms × 30 / 1000⌉ = 8 ticks, each in the
    // ticks whose remainder by 8 is its stable id: in ticks 0 to 9, 0 and 8 for the west's, id 0,
    // and 1 and 9 for the east's, id 1.
    let units = MatchUnits::of_world(fixed.runner().world());
    let [west, east] = [units.spawned_at(-8, -3), units.spawned_at(8, -3)];
    assert_eq!([west.get(), east.get()], [0, 1]);
    let failed = |tick, unit| ScriptCallFailed {
        tick: Tick::new(tick),
        unit: Some(unit),
        hook: Hook::OnThink,
        error: "script call raised \"no think\"".to_owned(),
    };
    assert_eq!(
        fixed.log().take::<ScriptCallFailed>(),
        [
            failed(0, west),
            failed(1, east),
            failed(8, west),
            failed(9, east)
        ]
    );
}

/// The state hash of each of the first 300 ticks of a lane match of `packages`.
fn hashes(packages: ModePackages) -> Vec<StateHash> {
    let session = FixedSession::new(packages, NonZeroU32::new(30).unwrap(), 2);
    let mut fixed = session.start();
    (0..300)
        .map(|_| {
            fixed.runner_mut().run_tick();
            fixed.runner().state_hash()
        })
        .collect()
}
