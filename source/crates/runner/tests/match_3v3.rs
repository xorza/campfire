//! The reference 3v3 as its packages hold it plays a match that replays to the same state hashes.

use std::num::NonZeroU32;
use std::path::PathBuf;

use campfire_capabilities::{
    Hook, InputValue, LaneWalker, ModeInput, ModeState, Owner, PlayerResources, ScriptFailures,
    StateValue, Team, UnitType,
};
use campfire_math::{Num, PlayerSlot, Vec3};
use campfire_package::{ModePackages, RELEASE};
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey};
use campfire_protocol::{
    Delegation, DelegationTerms, InputChain, SeedChain, SessionHeader, SessionLog, SessionTerms,
};
use campfire_runner::{Runner, Session};
use campfire_script::ScriptHost;
use campfire_sim::{EntityIndex, Position, StableId, StateHash};

/// The 3v3's slowest rate, which runs a match in the fewest ticks.
const TICK_HZ: NonZeroU32 = NonZeroU32::new(20).unwrap();
const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);
const SERVER_KEY: [u8; 32] = [8; 32];
/// BIP-340 signing without auxiliary randomness is deterministic, so every run signs alike.
const AUX: [u8; 32] = [0; 32];
const PLAYERS: u32 = 6;
const HEROES: [&str; 6] = [
    "hero-cinder",
    "hero-gale",
    "hero-husk",
    "hero-kensho",
    "hero-rime",
    "hero-veil",
];

fn moba() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/moba"))
}

fn packages() -> ModePackages {
    ModePackages::from_dir(&moba().join("modes/3v3")).unwrap_or_else(|error| panic!("{error}"))
}

fn key(byte: u32) -> Keypair {
    let secret = SecretKey::from_byte_array(&[u8::try_from(byte).unwrap(); 32]).unwrap();
    Keypair::from_secret_key(&Secp256k1::new(), &secret)
}

/// Player `slot`'s session key.
fn session_key(slot: u32) -> Keypair {
    key(20 + slot)
}

fn terms(packages: &ModePackages) -> SessionTerms {
    SessionTerms {
        server_key: SERVER_KEY,
        tick_hz: TICK_HZ,
        max_input_delay: 10,
        max_input_lead: 10,
        max_payload_len: 256,
        max_inputs_per_tick: 4,
        seed_commitment: SEED_CHAIN.commitment(),
        release: RELEASE.to_owned(),
        mode: Session::mode_in_terms(packages),
        dependencies: Session::dependencies_in_terms(packages),
    }
}

/// Player `slot`'s delegation in the session of `terms`.
fn delegation(slot: u32, terms: &SessionTerms) -> Delegation {
    let delegated = DelegationTerms {
        session_key: session_key(slot).x_only_public_key().0,
        server_key: SERVER_KEY,
        session_id: terms.session_id(),
        seed_contribution: [u8::try_from(slot).unwrap(); 32],
        expiration: 1_700_086_400,
    };
    Delegation::sign(
        &Secp256k1::new(),
        &key(10 + slot),
        &delegated,
        1_700_000_000,
        &AUX,
    )
}

fn log(terms: SessionTerms) -> SessionLog {
    let players = (0..PLAYERS).map(|slot| delegation(slot, &terms)).collect();
    SessionLog::new(SessionHeader { terms, players }).unwrap()
}

#[derive(Debug)]
struct Run {
    runner: Runner,
    hashes: Vec<StateHash>,
    /// Each unit after the tick the heroes spawn in, and after the one the first wave spawns in.
    at_pick_end: Vec<Unit>,
    at_first_wave: Vec<Unit>,
    /// The units whose calls failed, of every tick.
    failed: Vec<StableId>,
}

/// A unit as a test sees it: its team, its unit type, where it stands, who controls it, and
/// whether it walks a lane.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Unit {
    team: Team,
    kind: UnitType,
    pos: Position,
    controller: Option<u32>,
    walks: bool,
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
                walks: unit.contains::<LaneWalker>(),
            })
        })
        .collect()
}

/// A match of `ticks` ticks in which each player picks a hero and two spells before tick 0.
fn run(packages: &ModePackages, ticks: u64) -> Run {
    let terms = terms(packages);
    let mut runner = Runner::new(log(terms.clone()), SEED_CHAIN.seed(0), packages)
        .unwrap_or_else(|error| panic!("{error}"));
    let secp = Secp256k1::new();
    let mut applied = Vec::new();
    for slot in 0..PLAYERS {
        let mut chain =
            InputChain::new(PlayerSlot::new(slot), delegation(slot, &terms).chain_root());
        let payload = ModeInput::payload(&[
            ModeInput {
                name: "hero",
                value: InputValue::String(HEROES[slot as usize]),
            },
            ModeInput {
                name: "spells",
                value: InputValue::StringList(vec!["haste", "mend"]),
            },
        ]);
        let input = chain.extend(0, &payload);
        let signature = chain.sign(&secp, &session_key(slot), terms.session_id(), &AUX);
        runner.record([input], &signature, &mut applied).unwrap();
    }
    let mut run = Run {
        runner,
        hashes: Vec::new(),
        at_pick_end: Vec::new(),
        at_first_wave: Vec::new(),
        failed: Vec::new(),
    };
    for tick in 0..ticks {
        run.runner.run_tick();
        run.hashes.push(run.runner.state_hash());
        let failures = run.runner.world().non_send::<ScriptFailures>();
        for failure in failures.get() {
            assert_eq!(failure.hook, Hook::Think, "{failure:?}");
            run.failed.push(failure.unit.unwrap());
        }
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
    let packages = packages();
    let run = run(&packages, 2500);
    let runner = &run.runner;
    let world = runner.world();
    // State in the order of its fields' names: first_blood, then phase.
    let phase = &world.resource::<ModeState>().get()[1];
    assert_eq!(phase, &StateValue::Text("play".to_owned()));
    // Each script file compiles once, however many abilities or unit types run it: the mode's 4,
    // the six heroes' 5, 4, 5, 5, 5 and 5, and the spells' 6 make 39.
    assert_eq!(world.non_send::<ScriptHost>().compiled(), 39);

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
    assert_eq!(heroes, (0..PLAYERS).map(hero).collect::<Vec<_>>());
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
    let gold = world.resource::<PlayerResources>();
    for slot in 0..PLAYERS {
        assert_eq!(
            gold.amount(PlayerSlot::new(slot), "gold"),
            104,
            "player {slot}"
        );
    }
    // Only camps, 20 to 24, fail: their AI reads `unit.spawn_pos`, which the release does not
    // have yet.
    assert!(!run.failed.is_empty());
    assert!(run.failed.iter().all(|id| (20..25).contains(&id.get())));
    let hashes = &run.hashes;

    let mut file = Vec::new();
    runner.log().encode(&mut file);
    let decoded = SessionLog::decode(&file).unwrap();
    let mut replay = Runner::new(decoded.rewound(), SEED_CHAIN.seed(0), &packages).unwrap();
    for (tick, live) in hashes.iter().enumerate() {
        replay.run_tick();
        assert_eq!(replay.state_hash(), *live, "tick {tick}");
    }
}
