//! The reference 3v3 as its packages hold it plays a match with no failed call that replays to the
//! same state hashes.

use campfire_capabilities::{
    ActionSlot, ActionSlots, ModeState, Owner, PathWalker, PlayerResources, ResourceId,
    ScriptFailures, SlotKind, StateValue, Team, UnitType,
};
use campfire_math::{Num, PlayerSlot, Vec3};
use campfire_protocol::SessionLog;
use campfire_runner::{Golden, Reference3v3, Runner};
use campfire_script::ScriptHost;
use campfire_sim::{EntityIndex, Position, StateHash};

#[derive(Debug)]
struct Run {
    runner: Runner,
    hashes: Vec<StateHash>,
    golden: Golden,
    /// Each unit after the tick the heroes spawn in, and after the one the first wave spawns in.
    at_pick_end: Vec<Unit>,
    at_first_wave: Vec<Unit>,
}

/// A unit as a test sees it: its team, its unit type, where it stands, who controls it, whether
/// it walks a lane, and its ability slots.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Unit {
    team: Team,
    kind: UnitType,
    pos: Position,
    controller: Option<u32>,
    walks: bool,
    slots: Vec<ActionSlot>,
}

/// The pick lasts 60 s, 1200 ticks, set at the start: it ends in tick 1199. The first wave comes
/// 60 s later, in tick 2399.
const PICK_END: u64 = 1199;
const FIRST_WAVE: u64 = 2399;

fn units(runner: &Runner) -> Vec<Unit> {
    let world = runner.world();
    world
        .resource::<EntityIndex>()
        .iter()
        .filter_map(|(_, entity)| {
            let unit = world.entity(entity);
            Some(Unit {
                team: *unit.get::<Team>()?,
                kind: *unit.get::<UnitType>()?,
                pos: *unit.get::<Position>()?,
                controller: unit.get::<Owner>().map(|owner| owner.slot().get()),
                walks: unit.contains::<PathWalker>(),
                slots: unit
                    .get::<ActionSlots>()
                    .map_or_else(Vec::new, |slots| slots.iter().collect()),
            })
        })
        .collect()
}

/// A match of `ticks` ticks in which each player picks a hero and two spells before tick 0.
fn run(reference: &Reference3v3, ticks: u64) -> Run {
    let runner = reference.start();
    let mut run = Run {
        runner,
        hashes: Vec::new(),
        golden: Golden::new(reference.packages(), Reference3v3::PLAYERS),
        at_pick_end: Vec::new(),
        at_first_wave: Vec::new(),
    };
    for tick in 0..ticks {
        run.runner.run_tick();
        run.hashes.push(run.runner.state_hash());
        run.golden.record(&run.runner);
        let failures = run.runner.world().non_send::<ScriptFailures>();
        assert!(
            failures.get().is_empty(),
            "tick {tick}: {:?}",
            failures.get()
        );
        match tick {
            PICK_END => run.at_pick_end = units(&run.runner),
            FIRST_WAVE => run.at_first_wave = units(&run.runner),
            _ => {}
        }
    }
    run.runner.reveal_seed();
    run
}

fn ground(x: i64, z: i64) -> Position {
    let meters = |value| Num::from_int(value).unwrap();
    Position::new(Vec3::new(meters(x), Num::ZERO, meters(z))).unwrap()
}

#[test]
fn a_3v3_match_replays_to_the_same_hashes() {
    let reference = Reference3v3::load();
    let run = run(&reference, 2500);
    run.golden.check("3v3");
    let runner = &run.runner;
    let world = runner.world();
    // State in the order of its fields' names: first_blood, then phase.
    let phase = &world.resource::<ModeState>().get()[1];
    assert_eq!(phase, &StateValue::Text("play".to_owned()));
    // Each script file compiles once, however many abilities or unit types run it: the mode's 4,
    // the six heroes' 5, 4, 5, 5, 4 and 4, and the spells' 6 make 37.
    assert_eq!(world.non_send::<ScriptHost>().compiled(), 37);

    // The map's 14 structures from the start; at the pick's end, the 6 heroes at their teams'
    // spawns, slots 0 to 2 north and 3 to 5 south, and the 5 neutral camps.
    let hero = |slot: u32| {
        let team = u8::from(slot >= 3);
        let z = if team == 0 { -60 } else { 60 };
        (Team::new(team), ground(0, z), Some(slot))
    };
    let heroes: Vec<_> = run.at_pick_end[14..20]
        .iter()
        .map(|unit| (unit.team, unit.pos, unit.controller))
        .collect();
    assert_eq!(
        heroes,
        (0..Reference3v3::PLAYERS).map(hero).collect::<Vec<_>>()
    );
    // Each hero's slots, kind after kind: its three basic abilities and its ultimate, unlearned,
    // then the two spells its player chose, haste and mend, learned from the spawn: the same
    // two abilities for every hero; and last its weapon, learned from the spawn.
    let [basic, ultimate, spell, weapon] = [0, 1, 2, 3].map(SlotKind::new);
    let spells = &run.at_pick_end[14].slots[4..6];
    for unit in &run.at_pick_end[14..20] {
        let slots: Vec<_> = unit
            .slots
            .iter()
            .map(|slot| (slot.kind, slot.rank))
            .collect();
        let kinds = [
            (basic, 0),
            (basic, 0),
            (basic, 0),
            (ultimate, 0),
            (spell, 1),
            (spell, 1),
            (weapon, 1),
        ];
        assert_eq!(slots, kinds);
        assert_eq!(&unit.slots[4..6], spells);
    }
    assert_ne!(spells[0].action, spells[1].action);
    let camps: Vec<_> = run.at_pick_end[20..]
        .iter()
        .map(|unit| (unit.team, unit.pos))
        .collect();
    let neutral = Team::new(2);
    assert_eq!(
        camps,
        [
            (neutral, ground(-18, -12)),
            (neutral, ground(18, -12)),
            (neutral, ground(-18, 12)),
            (neutral, ground(18, 12)),
            (neutral, ground(0, 0)),
        ]
    );
    // The first wave: on each lane, west then east, each team's six creeps at its end: 24.
    let wave = &run.at_first_wave[25..];
    let seen: Vec<_> = wave.iter().map(|unit| (unit.team, unit.pos)).collect();
    let ends = [(0, -6, -50), (1, -6, 50), (0, 6, -50), (1, 6, 50)];
    let expected: Vec<_> = ends
        .iter()
        .flat_map(|&(team, x, z)| [(Team::new(team), ground(x, z)); 6])
        .collect();
    assert_eq!(seen, expected);
    assert!(wave.iter().all(|unit| unit.walks));
    // Melee creeps first, then casters, both types of the wave list.
    assert_eq!(wave[0].kind, wave[1].kind);
    assert_ne!(wave[0].kind, wave[5].kind);

    // Income: 8 gold every 5 s from the pick's end, 100 ticks, in ticks 1299 to 2499: 13 times.
    let amounts = world.resource::<PlayerResources>();
    let gold = ResourceId::of(&reference.packages().data().resources, "gold").unwrap();
    for slot in 0..Reference3v3::PLAYERS {
        assert_eq!(
            amounts.amount(PlayerSlot::new(slot), gold),
            104,
            "player {slot}"
        );
    }
    let hashes = &run.hashes;

    let mut file = Vec::new();
    runner.log().encode(&mut file);
    let decoded = SessionLog::decode(&file).unwrap();
    let mut replay = Runner::new(
        decoded.rewound(),
        Reference3v3::seed(),
        reference.packages(),
    )
    .unwrap();
    for (tick, live) in hashes.iter().enumerate() {
        replay.run_tick();
        assert_eq!(replay.state_hash(), *live, "tick {tick}");
    }
}
