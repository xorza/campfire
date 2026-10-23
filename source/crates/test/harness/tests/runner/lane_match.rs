//! The test lane mode plays 30 s to its golden record, whatever its heroes' text says: its walker
//! learns with its points, and fells creeps for their experience. Its waves trade to an end
//! before the next, and a wave with none to meet falls to the tower it strikes.

use std::num::NonZeroU64;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_actions::ActionSlots;
use campfire_capabilities::{Directive, Order};
use campfire_combat::{Combat, DamageCause};
use campfire_common::{DeclaredName, PlayerSlot, StateHash, Tick};
use campfire_harness::{
    ArchetypeWatch, FixedMatch, FixedSession, MatchProbe, MatchRecord, MatchUnits, ProbeFile,
    ProbeRefusal, ProbeSpec, ProbeUnits,
};
use campfire_math::Num;
use campfire_navigation::PathWalker;
use campfire_orders::OrdersDirective;
use campfire_package::ModePackages;
use campfire_production::Production;
use campfire_progression::{Experience, Points};
use campfire_runner::ScriptCallFailed;
use campfire_sim::EntityIndex;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{MatchRate, SimWorld, StableId};
use campfire_stats::Level;
use campfire_store::InputFile;
use campfire_store::internals::TestDir;
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_units::{Dead, Deaths, StatusTags, Units};
use campfire_units::{GroundPoint, Life, MoveStep, Owner, UnitTypeBook};
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

/// A match of the lane mode whose one player's walker walks from tick 0 to the map's edge, 8 m off
/// the lane, out of every creep's and tower's reach.
fn quiet_lane() -> FixedMatch {
    let mut fixed = FixedSession::new(TestMode::Lane.packages(), 1).start();
    let walker = MatchUnits::of(fixed.game()).hero(0);
    let edge = GroundPoint {
        x: Num::ZERO,
        z: Num::from_int(8).unwrap(),
    };
    order(&mut fixed, walker, 0, OrdersDirective::Move { to: edge });
    fixed
}

/// The units of `team` in `world` that walk, as `walks`, and have no owner: its creeps, or its
/// tower.
fn unowned(world: &World, team: u8, walks: bool) -> Vec<Entity> {
    world
        .resource::<EntityIndex>()
        .iter()
        .map(|(_, entity)| entity)
        .filter(|&entity| {
            let unit = world.entity(entity);
            unit.get::<Team>() == Some(&Team::new(team))
                && unit.contains::<MoveStep>() == walks
                && !unit.contains::<Owner>()
                && unit.contains::<UnitType>()
        })
        .collect()
}

/// 30 s between waves at 30 ticks a second.
const WAVE_TICKS: u64 = 900;

#[test]
fn two_equal_waves_trade_to_an_end_before_the_next_wave() {
    // The waves spawn 32 m apart and close at 7.5 m/s: they meet about 4 s later. The lane is a
    // mirror, so each creep has its twin: the two front creeps fall to twelve strikes of 25, six
    // from each rear creep, then the two rear creeps to twelve more, 24 ticks apart, 9.6 s. All
    // four fall well before the next wave, the twins in the same tick, and neither wave is left
    // to push: a hero tips the balance.
    let mut fixed = quiet_lane();
    let mut tick = 0;
    while unowned(fixed.game(), 0, true).is_empty() {
        fixed.step();
        tick += 1;
    }
    let spawned = tick;
    let mut gone = [None; 2];
    while gone.iter().any(Option::is_none) {
        assert!(tick - spawned < WAVE_TICKS, "the fight ends");
        fixed.step();
        tick += 1;
        for (team, gone) in (0..).zip(&mut gone) {
            if gone.is_none() && unowned(fixed.game(), team, true).is_empty() {
                *gone = Some(tick);
            }
        }
    }
    assert_eq!(gone[0], gone[1]);
}

#[test]
fn a_wave_with_no_wave_to_meet_strikes_the_tower_and_falls_to_it() {
    // The west wave goes as it spawns. The east wave walks into the west tower's reach, strikes
    // it for 25 a strike, and falls to it: two of its strikes of 150 kill a creep of 300. All
    // this before the next wave.
    let mut fixed = quiet_lane();
    let mut tick = 0;
    while unowned(fixed.game(), 0, true).is_empty() {
        fixed.step();
        tick += 1;
    }
    let spawned = tick;
    for creep in unowned(fixed.game(), 0, true) {
        fixed.game_mut().despawn(creep);
    }
    let west = unowned(fixed.game(), 0, false)[0];
    while !unowned(fixed.game(), 1, true).is_empty() {
        assert!(tick - spawned < WAVE_TICKS, "the tower kills the wave");
        fixed.step();
        tick += 1;
    }
    let full = Num::from_int(1500).unwrap();
    let lost = full - fixed.game().get::<Life>(west).unwrap().current();
    let strike = Num::from_int(25).unwrap();
    // The wave lands two strikes before the tower fells it.
    assert_eq!(lost, strike * 2, "{lost:?} of {full:?}");
}

/// The mode's scripted match, which `source/.config/nextest.toml` exempts from the bound of 1 s.
mod scripted_match {
    use campfire_progression::ProgressionDirective;

    use super::*;

    #[test]
    fn the_lane_match_plays_to_its_golden_record() {
        let session = FixedSession::new(TestMode::Lane.packages(), 2);
        let mut fixed = session.start();
        let mut record =
            MatchRecord::of(fixed.game_mut(), session.packages(), session.player_slots());
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
            fixed.step();
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
            for death in world.resource::<Deaths>() {
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
        2,
    );
    let mut fixed = session.start();
    for _ in 0..10 {
        fixed.step();
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
    let session = FixedSession::new(packages, 2);
    let mut fixed = session.start();
    (0..300)
        .map(|_| {
            fixed.step();
            fixed.game().state_hash()
        })
        .collect()
}

#[test]
fn a_kill_gives_its_killer_the_victims_experience_and_an_allys_kill_gives_none() {
    // The first wave spawns in tick 0. The walker fells a creep of the other team: it takes the
    // creep's 60; it fells one of its own team, an ally's kill: it takes none.
    let session = FixedSession::new(TestMode::Lane.packages(), 2);
    let mut fixed = session.start();
    fixed.step();
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
        fixed.step();
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
        let session = FixedSession::new(tree.load().unwrap(), 2);
        let mut fixed = session.start();
        fixed.step();
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
        fixed.step();
        Combat::damage(
            fixed.game_mut(),
            Some(walker),
            enemy,
            100_000,
            DamageCause::Effect,
        );
        fixed.step();
        Progress::of(&fixed, walker).xp
    };
    assert_eq!(xp_of_a_site_kill(true), Num::ZERO);
    assert_eq!(xp_of_a_site_kill(false), Num::int(60));
}

/// The stable ids of the units the table of `probe`'s last record holds, in its order.
fn probed(probe: &MatchProbe) -> Vec<u64> {
    let block = probe.text().trim_end().rsplit("\n\n").next().unwrap();
    let rows = block
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("id"));
    rows.skip(1)
        .map(|row| row.split_whitespace().next().unwrap().parse().unwrap())
        .collect()
}

#[test]
fn a_probe_prints_the_units_it_selects_at_its_period() {
    // The walker spawns at (0, −2) and walks to (0, 8) from tick 0 at 7.5 m/s, 0.25 m a tick at
    // 30 ticks a second: after tick k it stands at z = −2 + 0.25 (k + 1), −1.75 after tick 0 and
    // 5.75 after tick 30. The west tower stands at (−8, −3) with 1500 life, and has no owner,
    // destination, route or progress. A period of 30 ticks writes ticks 0 and 30 of 31.
    let mut fixed = quiet_lane();
    let world = fixed.game();
    let hero = MatchUnits::of(world).hero(0);
    let tower = *world.get::<StableId>(unowned(world, 0, false)[0]).unwrap();
    let every = NonZeroU64::new(30).unwrap();
    let probe = MatchProbe::new(every, ProbeUnits::Ids(vec![tower, hero]));
    let dir = TestDir::new();
    let file = dir.path("lane.probe");
    fixed.probe(ProbeFile { probe, file });
    for _ in 0..31 {
        fixed.step();
    }
    let probe = fixed.take_probe().unwrap();
    assert_eq!(
        probe.probe.text(),
        "\
tick 0, 0 s
player slot 0: 0 idle, 1 walking (0 stuck), 0 working, 0 fighting, 0 acting
id  type         slot  team   x      z  doing  target  destination  goal  stuck       life
 0  tower           -     0  -8     -3  idle   -       -            -         -  1500/1500
 2  hero-walker     0     0   0  -1.75  walk   -       0, 8         0, 8      0    600/600

tick 30, 1 s
player slot 0: 0 idle, 1 walking (0 stuck), 0 working, 0 fighting, 0 acting
id  type         slot  team   x     z  doing  target  destination  goal  stuck       life
 0  tower           -     0  -8    -3  idle   -       -            -         -  1500/1500
 2  hero-walker     0     0   0  5.75  walk   -       0, 8         0, 8      0    600/600

"
    );

    // The match writes the probe it holds as it drops: its one tick, 31, of a period of 1.
    let every = NonZeroU64::new(1).unwrap();
    let file = dir.path("dropped.probe");
    let units = ProbeUnits::Ids(vec![hero]);
    let probe = MatchProbe::new(every, units.clone());
    fixed.probe(ProbeFile {
        probe,
        file: file.clone(),
    });
    fixed.step();
    let mut alone = MatchProbe::new(every, units);
    alone.record(fixed.game());
    let written = alone.text().to_owned();
    assert!(
        written.starts_with("tick 31, 1 s and 1 ticks\n"),
        "{written}"
    );

    // A spec refuses a period of no whole count of the lane's 30 ticks a second, 50 ms being 1.5
    // ticks, and a type the lane has not.
    let types = fixed.game().get_resource::<UnitTypeBook>();
    let hz = fixed.game().resource::<MatchRate>().hz();
    let refused = |text: &str| {
        text.parse::<ProbeSpec>()
            .unwrap()
            .probe(types, hz)
            .unwrap_err()
    };
    assert_eq!(
        refused("50:all"),
        ProbeRefusal::Period(NonZeroU64::new(50).unwrap())
    );
    let nowhere = DeclaredName::new("nowhere").unwrap();
    assert_eq!(refused("100:type=nowhere"), ProbeRefusal::NoType(nowhere));

    // Each selection, of a probe of every tick: the lane holds two towers, 0 and 1, the walker,
    // 2, and the first wave's creeps, 3 and 4 of team 0 and 5 and 6 of team 1.
    let world = fixed.game();
    let creep = Units::type_named(world, "creep").unwrap();
    let every = NonZeroU64::new(1).unwrap();
    for (units, ids) in [
        (ProbeUnits::All, &[0, 1, 2, 3, 4, 5, 6][..]),
        (ProbeUnits::Owner(PlayerSlot::new(0)), &[2]),
        (ProbeUnits::Team(Team::new(1)), &[1, 5, 6]),
        (ProbeUnits::Type(creep), &[3, 4, 5, 6]),
        // In any order, each once.
        (ProbeUnits::Ids(vec![hero, tower, hero]), &[0, 2]),
    ] {
        let mut probe = MatchProbe::new(every, units.clone());
        probe.record(world);
        assert_eq!(probed(&probe), ids, "{units:?}");
    }
    drop(fixed);
    let read = InputFile::read_text(&file, 1 << 16).unwrap();
    assert_eq!(read, written);
}
