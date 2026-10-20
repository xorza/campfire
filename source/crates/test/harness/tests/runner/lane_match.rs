//! The test lane mode plays 30 s to its golden record, whatever its heroes' text says: its walker
//! learns with its points, and fells creeps for their experience.

use std::num::NonZeroU32;

use campfire_actions::ActionSlots;
use campfire_capabilities::{Directive, Order};
use campfire_combat::{Combat, DamageCause};
use campfire_common::{StateHash, Tick};
use campfire_harness::{ArchetypeWatch, FixedMatch, FixedSession, MatchRecord, MatchUnits};
use campfire_math::Num;
use campfire_navigation::PathWalker;
use campfire_package::ModePackages;
use campfire_production::Production;
use campfire_progression::{Experience, Points};
use campfire_runner::{ScriptCallFailed, SessionWorld};
use campfire_script::ScriptFailures;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{SimWorld, StableId};
use campfire_stats::Level;
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_units::{Dead, Deaths, StatusTags, Units};
use campfire_values::{Rank, Team, TrackId, UnitType};

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
    fn of(fixed: &FixedMatch, hero: StableId) -> Self {
        let world = fixed.game();
        let unit = world.entity_ref_of(hero).unwrap();
        let slots = unit.get::<ActionSlots>().unwrap();
        let track = TrackId::new(0);
        Self {
            ranks: [0, 1, 2, 3].map(|slot| Rank::count(slots.slot(slot).unwrap().rank)),
            points: unit.get::<Points>().unwrap().get(),
            xp: unit.get::<Experience>().unwrap().get(track).unwrap().xp,
            level: unit.get::<Level>().unwrap().get(),
        }
    }
}

/// Player 0 sends `directive` for the walker, stamped for `tick`.
fn order(fixed: &mut FixedMatch, walker: StableId, tick: u64, directive: impl Into<Directive>) {
    let payload = Order::payload(&[Order::one(walker, directive)]);
    fixed.send(0, Tick::new(tick), &payload);
}

/// The ticks the lane match plays, and the one its east creeps reach the walker in.
const TICKS: u64 = 900;
const MEET: u64 = 140;

/// The mode's scripted match, which `source/.config/nextest.toml` exempts from the bound of 1 s.
mod scripted_match {
    use campfire_orders::OrdersDirective;
    use campfire_progression::ProgressionDirective;

    use super::*;

    #[test]
    fn the_lane_match_plays_to_its_golden_record() {
        let session = FixedSession::new(TestMode::Lane.packages(), NonZeroU32::new(30).unwrap(), 2);
        let mut fixed = session.start();
        let mut record = MatchRecord::of(fixed.game_mut(), session.packages(), session.seats());
        let walker = MatchUnits::of(fixed.game()).hero(0);
        // The first wave's creeps of the east, which spawn in tick 0, walk to the walker and strike
        // it as they meet; the west's strike the runner, so the walker alone fells these.
        let mut east = Vec::new();
        let mut felled = Vec::new();
        let mut seen = Vec::new();
        let mut watch = ArchetypeWatch::of(fixed.game_mut());
        for tick in 0..TICKS {
            // Tick 1: the walker learns its first ability with its spawn point. As they meet, it
            // attacks the first east creep, and the second once the first falls. Once both fell, it
            // tries its ultimate, then a second rank of its first ability.
            let next = felled.len();
            match tick {
                1 => order(
                    &mut fixed,
                    walker,
                    tick,
                    ProgressionDirective::Learn { slot: 0 },
                ),
                MEET => order(
                    &mut fixed,
                    walker,
                    tick,
                    OrdersDirective::Attack { target: east[0] },
                ),
                _ if next == 1 && felled[0] + 1 == tick => {
                    order(
                        &mut fixed,
                        walker,
                        tick,
                        OrdersDirective::Attack { target: east[1] },
                    );
                }
                _ if next == 2 && felled[1] + 1 == tick => {
                    order(
                        &mut fixed,
                        walker,
                        tick,
                        ProgressionDirective::Learn { slot: 3 },
                    );
                    order(
                        &mut fixed,
                        walker,
                        tick,
                        ProgressionDirective::Learn { slot: 0 },
                    );
                }
                _ => {}
            }
            fixed.game_mut().run_tick_alone();
            record.tick(fixed.game_mut());
            watch.tick(fixed.game(), tick);
            if tick == 0 {
                east = MatchUnits::of(fixed.game())
                    .all()
                    .filter(|(_, unit)| {
                        unit.contains::<PathWalker>() && unit.get::<Team>() == Some(&Team::new(1))
                    })
                    .map(|(id, _)| id)
                    .collect();
            }
            let world = fixed.game();
            let failures = world.resource::<ScriptFailures>();
            assert!(
                failures.get().is_empty(),
                "tick {tick}: {:?}",
                failures.get()
            );
            for death in world.resource::<Deaths>().iter() {
                if east.contains(&death.fallen.id) {
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
        record.check("lane");
        watch.assert_kept();
        // After tick 1: its first ability at rank 1, for its one point. Each creep it fells gives
        // it 60 experience: 120 after the second, past level 2's 100, which gives a point. Its
        // ultimate needs level 3, and its first ability's rank 2 level 2: the point goes to that.
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
    let plain = TestMode::Lane.packages();
    let reworded = PackageTree::of(TestMode::Lane)
        .edit(
            "heroes/walker/locale/en.ftl",
            Edit::Replace("hero-name = Walker", "hero-name = Wanderer"),
        )
        .edit(
            "heroes/walker/locale/de.ftl",
            Edit::Create("hero-name = Wanderer\n"),
        )
        .packages();
    let walker = |packages: &ModePackages| {
        let walker = packages
            .packages()
            .find(|view| view.package.header.name == "hero-walker");
        walker.unwrap().package.fingerprint()
    };
    assert_ne!(walker(&plain), walker(&reworded));
    assert_eq!(hashes(plain), hashes(reworded));
}

#[test]
fn a_failed_script_call_logs_its_tick_unit_hook_and_why() {
    let throws = Edit::Replace(
        "fn on_think(ctx, tower) {\n",
        "fn on_think(ctx, tower) {\n    throw \"no think\";\n",
    );
    let session = FixedSession::new(
        PackageTree::of(TestMode::Lane)
            .edit("modes/lane/scripts/tower_ai.rhai", throws)
            .packages(),
        NonZeroU32::new(30).unwrap(),
        2,
    );
    let mut fixed = session.start();
    for _ in 0..10 {
        fixed.game_mut().run_tick_alone();
    }
    // The towers, the map's first units, think every ⌈250 ms × 30 / 1000⌉ = 8 ticks, each in the
    // ticks whose remainder by 8 is its stable id: in ticks 0 to 9, 0 and 8 for the west's, id 0,
    // and 1 and 9 for the east's, id 1.
    let units = MatchUnits::of(fixed.game());
    let [west, east] = [units.spawned_at((-8, 0, -3)), units.spawned_at((8, 0, -3))];
    assert_eq!([west.get(), east.get()], [0, 1]);
    let failed = |tick, unit| ScriptCallFailed {
        tick: Tick::new(tick),
        unit: Some(unit),
        hook: "on_think".to_owned(),
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
            fixed.game_mut().run_tick_alone();
            fixed.game().state_hash()
        })
        .collect()
}

#[test]
fn a_kill_gives_its_killer_the_victims_experience_and_an_allys_kill_gives_none() {
    // The first wave spawns in tick 0. The walker fells a creep of the other team: it takes the
    // creep's 60; it fells one of its own team, an ally's kill: it takes none.
    let session = FixedSession::new(TestMode::Lane.packages(), NonZeroU32::new(30).unwrap(), 2);
    let mut fixed = session.start();
    fixed.game_mut().run_tick_alone();
    let walker = MatchUnits::of(fixed.game()).hero(0);
    let creep = Units::type_named(fixed.game(), "creep").unwrap();
    let team =
        |fixed: &FixedMatch, id| *MatchUnits::of(fixed.game()).unit(id).get::<Team>().unwrap();
    let ours = team(&fixed, walker);
    let creep_of = |fixed: &FixedMatch, own: bool| {
        let units = MatchUnits::of(fixed.game());
        let found = units.all().find(|(_, unit)| {
            unit.get::<UnitType>() == Some(&creep)
                && (unit.get::<Team>() == Some(&ours)) == own
                && !unit.contains::<Dead>()
        });
        found.map(|(id, _)| id).unwrap()
    };
    let fell = |fixed: &mut FixedMatch, victim| {
        Combat::damage(
            fixed.game_mut(),
            Some(walker),
            victim,
            100_000,
            DamageCause::Effect,
        );
        fixed.game_mut().run_tick_alone();
        Progress::of(fixed, walker).xp
    };
    let enemy = creep_of(&fixed, false);
    assert_eq!(fell(&mut fixed, enemy), Num::int(60));
    let ally = creep_of(&fixed, true);
    assert_eq!(fell(&mut fixed, ally), Num::int(60));
}

/// A victim the mode's `[experience]` victim filter leaves out gives no experience: an enemy creep
/// marked as a site, `constructing`, gives the walker none under `all:!constructing`, and its 60
/// with no filter.
#[test]
fn a_victim_the_experience_filter_leaves_out_gives_none() {
    let xp_of_a_site_kill = |filtered: bool| {
        let mut tree = PackageTree::of(TestMode::Lane);
        if filtered {
            tree.edit(
                "modes/lane/data/mode.toml",
                Edit::Set("experience.victim", "\"all:!constructing\""),
            );
        }
        let session = FixedSession::new(tree.load().unwrap(), NonZeroU32::new(30).unwrap(), 2);
        let mut fixed = session.start();
        fixed.game_mut().run_tick_alone();
        let walker = MatchUnits::of(fixed.game()).hero(0);
        let creep = Units::type_named(fixed.game(), "creep").unwrap();
        let ours = *MatchUnits::of(fixed.game())
            .unit(walker)
            .get::<Team>()
            .unwrap();
        let enemy = MatchUnits::of(fixed.game())
            .all()
            .find(|(_, unit)| {
                unit.get::<UnitType>() == Some(&creep) && unit.get::<Team>() != Some(&ours)
            })
            .map(|(id, _)| id)
            .unwrap();
        let site = StatusTags::from([Production::CONSTRUCTING]);
        fixed.game_mut().insert_parts(enemy, site);
        fixed.game_mut().run_tick_alone();
        Combat::damage(
            fixed.game_mut(),
            Some(walker),
            enemy,
            100_000,
            DamageCause::Effect,
        );
        fixed.game_mut().run_tick_alone();
        Progress::of(&fixed, walker).xp
    };
    assert_eq!(xp_of_a_site_kill(true), Num::ZERO);
    assert_eq!(xp_of_a_site_kill(false), Num::int(60));
}
