//! The first half of the Stage 2 gate, without the network: a match of the test lane mode run from
//! its session log and the replay of that log in a bare `World` agree on the state hash after
//! every tick, and so does the replay of the log's file with the packages a verifier holds. So
//! does the reference 3v3's, whose players learn ranks.

use std::collections::BTreeMap;
use std::fs;
use std::num::NonZeroU32;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use campfire_capabilities::{Action, ActionSlots, Destination, Order, PoolId, Pools, Projectile};
use campfire_common::{Fingerprint, PlayerSlot, StateHash, Tick, Ticks};
use campfire_log::LogEvent;
use campfire_log::internals::LogCheck;
use campfire_math::{Num, Vec3};
use campfire_package::{ModePackages, PackageDir, PackageStore, StoreError};
use campfire_protocol::{
    AfterLeave, Applied, LeaveReason, Outcome, SeedError, ServerInput, ServerSeed, ServerSeeds,
    SessionLog, SessionResult, SessionTerms, SlotChange, SlotChangeKind, SlotPlan,
    SnapshotFingerprint, Taken,
};
use campfire_runner::internals::{FixedMatch, FixedSession, HashTrail, MatchUnits, Reference3v3};
use campfire_runner::{
    InputRules, ResultMismatch, Runner, ServerInputRefused, SlotRuleError, StartError, TermsError,
};
use campfire_sim::{EntityIndex, Position, StableId};
use campfire_verifier::{Replay, ReplayError, SnapshotCheckError, Verified};
use tempfile::TempDir;

/// The lane mode's life pool, `health`, the first of its pools by name.
const LIFE: PoolId = PoolId::FIRST;

/// Long enough for every order to apply, and for the hero to arrive.
const TICKS: u64 = 40;

/// A move order as the player sends it.
#[derive(Debug)]
struct Sent {
    /// The tick it arrives before.
    arrives: u64,
    stamp: u64,
    x: i64,
    z: i64,
    applied: Applied,
}

/// At a quarter meter a tick, straight lines take exactly 4 ticks a meter.
const ORDERS: [Sent; 3] = [
    // Toward (0, 5), from the spawn at (0, −2): 5.5 m in ticks 0 to 21.
    Sent {
        arrives: 0,
        stamp: 0,
        x: 0,
        z: 5,
        applied: Applied::At(Tick::new(0)),
    },
    // Stamped 2 ticks ahead; from (0, 3.5), 2.5 m to (−2, 5): ticks 22 to 31.
    Sent {
        arrives: 20,
        stamp: 22,
        x: -2,
        z: 5,
        applied: Applied::At(Tick::new(22)),
    },
    // 4 ticks after its stamp, past the max delay of 3: logged, never applied.
    Sent {
        arrives: 30,
        stamp: 26,
        x: 9,
        z: 9,
        applied: Applied::Late,
    },
];

fn packages() -> ModePackages {
    ModePackages::from_dir(&PackageDir::workspace("test/modes/lane")).unwrap()
}

/// Every file under `dir`, by its path from `dir`.
fn files_under(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut dirs = vec![PathBuf::new()];
    while let Some(at) = dirs.pop() {
        for entry in fs::read_dir(dir.join(&at)).unwrap() {
            let entry = entry.unwrap();
            let path = at.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                dirs.push(path);
            } else {
                files.insert(path, fs::read(entry.path()).unwrap());
            }
        }
    }
    files
}

fn store() -> PackageStore {
    PackageStore::scan(&PackageDir::workspace("")).unwrap()
}

/// A session of the test lane mode at its 30 ticks a second, for its one player, whose inputs
/// may come 3 ticks late or early.
fn session() -> FixedSession {
    let rules = InputRules {
        max_input_delay: Ticks::new(3),
        max_input_lead: Ticks::new(3),
        max_payload_len: 64,
        max_inputs_per_tick: 4,
    };
    FixedSession::with_rules(packages(), NonZeroU32::new(30).unwrap(), 1, rules)
}
#[derive(Debug, PartialEq, Eq)]
struct Hero {
    position: Position,
    destination: Destination,
}

/// Where player `slot`'s hero stands and walks to.
fn hero(runner: &Runner, slot: u32) -> Hero {
    let world = runner.world();
    let id = MatchUnits::of_world(world).hero(slot);
    let entity = world.resource::<EntityIndex>().get(id).unwrap();
    let hero = world.entity(entity);
    Hero {
        position: *hero.get::<Position>().unwrap(),
        destination: *hero.get::<Destination>().unwrap(),
    }
}

#[derive(Debug)]
struct Run {
    fixed: FixedMatch,
    /// The state after each tick.
    trail: HashTrail,
}

/// Runs a match in which the player sends `orders`.
fn run(orders: &[&Sent], ticks: u64) -> Run {
    let mut fixed = session().start();
    let mut trail = HashTrail::default();
    play(&mut fixed, orders, 0..ticks, &mut trail);
    fixed.runner_mut().reveal_seed();
    Run { fixed, trail }
}

/// Runs `ticks` of `fixed`, the player sending `orders` as they arrive, and records each tick's
/// state into `trail`.
fn play(fixed: &mut FixedMatch, orders: &[&Sent], ticks: Range<u64>, trail: &mut HashTrail) {
    for tick in ticks {
        for sent in orders.iter().filter(|sent| sent.arrives == tick) {
            let payload = move_order(fixed, 0, sent.x, sent.z);
            assert_eq!(
                fixed.send(0, Tick::new(sent.stamp), &payload),
                sent.applied,
                "{sent:?}"
            );
        }
        fixed.runner_mut().run_tick();
        trail.record(fixed.runner().world());
    }
}

/// The orders' match to tick 72, checkpointed with a full snapshot, written into `snapshot`,
/// before tick 40, its record's state hash `hash` when given, and ended with `outcome`, as the
/// mode ended it when none is given; the match, and its state after each tick.
fn checkpointed(snapshot: &mut Vec<u8>, hash: Option<StateHash>, outcome: Option<Outcome>) -> Run {
    let mut fixed = session().start();
    let mut trail = HashTrail::default();
    let orders = ORDERS.each_ref();
    play(&mut fixed, &orders, 0..40, &mut trail);
    let mut record = fixed.runner().checkpoint(snapshot).unwrap();
    record.state_hash = hash.unwrap_or(record.state_hash);
    let id = fixed.runner().log().session_id();
    let signature = FixedSession::checkpoint_signature(&record, id);
    fixed
        .runner_mut()
        .record_checkpoint(record, &signature)
        .unwrap();
    play(&mut fixed, &orders, 40..72, &mut trail);
    let runner = fixed.runner_mut();
    let result = outcome.map_or_else(|| runner.result(), |outcome| runner.result_as(outcome));
    let signature = FixedSession::result_signature(&result, id);
    runner.record_result(result, &signature).unwrap();
    runner.reveal_seed();
    Run { fixed, trail }
}

/// The replay of `fixed`'s log file, its state after each tick, and how it ended.
fn replayed(fixed: &FixedMatch) -> (Replay, HashTrail, Result<(), ReplayError>) {
    let decoded = SessionLog::decode(&encoded(fixed.runner().log())).unwrap();
    let mut replay = Replay::new(decoded, &store()).unwrap();
    let mut trail = HashTrail::default();
    let end = loop {
        match replay.run_tick() {
            Ok(true) => trail.record(replay.runner().world()),
            Ok(false) => break Ok(()),
            Err(error) => break Err(error),
        }
    };
    (replay, trail, end)
}

#[test]
fn a_log_checkpointed_at_tick_40_verifies_and_fails_at_the_checkpoint_with_another_hash() {
    // Two segments: the replay reaches the state the checkpoint records, the snapshot restores
    // to it, and the match runs on to the same hashes. The lane match has not ended by tick 72,
    // so the session aborts.
    let mut snapshot = Vec::new();
    let Run { fixed, trail: live } = checkpointed(&mut snapshot, None, None);
    let log = fixed.runner().log();
    let record = log.checkpoint_at(Tick::new(40)).unwrap().clone();
    assert_eq!(log.segment(), 1);
    assert_eq!(record.snapshot, SnapshotFingerprint::of(&snapshot));
    let ended = SessionResult {
        tick: Tick::new(72),
        outcome: Outcome::Aborted,
        state_hash: fixed.runner().state_hash(),
    };
    assert_eq!(log.result(), Some(&ended));
    let (replay, trail, end) = replayed(&fixed);
    assert_eq!(end, Ok(()));
    live.assert_same(&trail);
    // Seeds that reach segment 0 alone do not start the log of two.
    let first = ServerSeeds::new(0, FixedSession::seed(0));
    assert!(matches!(
        Runner::new(
            SessionLog::decode(&encoded(log)).unwrap().rewound(),
            first,
            &packages()
        ),
        Err(StartError::Seed(SeedError::NotRevealed))
    ));
    assert_eq!(replay.check_snapshot(&record, &snapshot), Ok(()));
    let mut other = snapshot.clone();
    *other.last_mut().unwrap() ^= 1;
    assert_eq!(
        replay.check_snapshot(&record, &other),
        Err(SnapshotCheckError::Fingerprint)
    );

    // The record's state hash changed: the replay stops at the checkpoint, and the snapshot
    // restores to another state than it records.
    let forged = StateHash::new([7; 32]);
    let Run { fixed, .. } = checkpointed(&mut snapshot, Some(forged), None);
    let (replay, _, end) = replayed(&fixed);
    let refused = ReplayError::Checkpoint {
        segment: 1,
        tick: Tick::new(40),
        logged: forged,
        replayed: record.state_hash,
    };
    assert_eq!(end, Err(refused));
    let restored = SnapshotCheckError::Hash {
        logged: forged,
        restored: record.state_hash,
    };
    let record = fixed.runner().log().checkpoint_at(Tick::new(40)).unwrap();
    assert_eq!(replay.check_snapshot(record, &snapshot), Err(restored));

    // A result that names a winner of a match that has none.
    let won = Outcome::Won { team: 0 };
    let Run { fixed, .. } = checkpointed(&mut snapshot, None, Some(won));
    let mismatch = ResultMismatch::Outcome {
        logged: won,
        ended: Outcome::Aborted,
    };
    assert_eq!(replayed(&fixed).2, Err(ReplayError::Result(mismatch)));
}

#[test]
fn run_and_replay_agree_on_every_tick() {
    let Run { fixed, trail: live } = run(&ORDERS.each_ref(), TICKS);
    let runner = fixed.runner();
    let arrived = Hero {
        position: Position::new(Vec3::new(Num::int(-2), Num::ZERO, Num::int(5))).unwrap(),
        destination: Destination::default(),
    };
    assert_eq!(hero(runner, 0), arrived);

    let decoded = SessionLog::decode(&encoded(runner.log())).unwrap();
    let mut replay = Replay::new(decoded, &store()).unwrap();
    let mut replayed = HashTrail::default();
    while replay.run_tick().unwrap() {
        replayed.record(replay.runner().world());
    }
    live.assert_same(&replayed);
    assert_eq!(hero(replay.runner(), 0), arrived);

    // Without the second order the hashes agree until it would apply, at tick 22, and differ
    // from then on: the hash sees the hero move.
    let without = run(&[&ORDERS[0], &ORDERS[2]], TICKS).trail;
    let (live, without) = (live.totals(), without.totals());
    let first_difference = live.iter().zip(without).position(|(a, b)| a != b);
    assert_eq!(first_difference, Some(22));
    assert!(live[22..].iter().zip(&without[22..]).all(|(a, b)| a != b));
}

/// The order that moves player `slot`'s hero to (`x`, `z`).
fn move_order(fixed: &FixedMatch, slot: u32, x: i64, z: i64) -> Vec<u8> {
    Order::payload(&[Order {
        unit: MatchUnits::of(fixed).hero(slot),
        action: Action::Move {
            x: Num::int(x),
            z: Num::int(z),
        },
    }])
}

/// The hero standing at (`x`, `z`), with nowhere to walk.
fn standing(x: i64, z: i64) -> Hero {
    Hero {
        position: Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap(),
        destination: Destination::default(),
    }
}

/// Player 0's leave of slot 0, the slot becoming `becomes`.
fn leave(becomes: AfterLeave) -> ServerInput {
    ServerInput::Leave {
        slot: PlayerSlot::new(0),
        reason: LeaveReason::Asked,
        becomes,
    }
}

/// Sends tick `tick`'s inputs of a lane match whose slot 1 the server's bot plays; the lane
/// mode's `[players]` is the default: no late join, no bot takeover, a leaver's slot reserved.
fn bot_and_returning_player(fixed: &mut FixedMatch, tick: u64) {
    match tick {
        // At a quarter meter a tick, the player's hero walks along z from (0, −2) toward
        // (0, 5), and the bot's 3 m along x from (0, 2) to (3, 2) in ticks 0 to 11.
        0 => {
            let payload = move_order(fixed, 0, 0, 5);
            fixed.send(0, Tick::new(0), &payload);
            let payload = move_order(fixed, 1, 3, 2);
            let order = ServerInput::Bot {
                slot: PlayerSlot::new(1),
                payload,
            };
            fixed.serve(order).unwrap();
        }
        // The mode refuses a leaver's slot to a bot, and a player in the bot's slot; neither is
        // logged. The player leaves, their slot reserved; their hero walks on, as no rule of the
        // mode acts on a leave.
        10 => {
            assert!(matches!(
                fixed.serve(leave(AfterLeave::Bot)),
                Err(ServerInputRefused::Rule(SlotRuleError::Leaver {
                    becomes: AfterLeave::Bot
                }))
            ));
            assert!(matches!(
                fixed.join(1),
                Err(ServerInputRefused::Rule(SlotRuleError::BotTakeover))
            ));
            assert_eq!(fixed.runner().log().next_place().index, 0);
            fixed.serve(leave(AfterLeave::Reserve)).unwrap();
        }
        // They come back, and turn their hero, at (0, 3.5) after 22 ticks, to (−2, 5): 2.5 m in
        // ticks 22 to 31.
        20 => {
            fixed.join(0).unwrap();
            let payload = move_order(fixed, 0, -2, 5);
            fixed.send(0, Tick::new(22), &payload);
        }
        _ => {}
    }
}

#[test]
fn a_log_with_a_bot_and_a_player_who_leaves_and_returns_replays_to_the_same_hashes() {
    let session = FixedSession::planned(
        packages(),
        NonZeroU32::new(30).unwrap(),
        InputRules::ROOMY,
        vec![SlotPlan::Player, SlotPlan::Bot],
    );
    let mut fixed = session.start();
    let mut live = HashTrail::default();
    for tick in 0..TICKS {
        bot_and_returning_player(&mut fixed, tick);
        fixed.runner_mut().run_tick();
        live.record(fixed.runner().world());
    }
    fixed.runner_mut().reveal_seed();
    let runner = fixed.runner();
    assert_eq!(
        [hero(runner, 0), hero(runner, 1)],
        [standing(-2, 5), standing(3, 2)]
    );
    let slot = PlayerSlot::new(0);
    let changes = [
        SlotChange {
            tick: Tick::new(10),
            slot,
            kind: SlotChangeKind::Left {
                becomes: AfterLeave::Reserve,
            },
        },
        SlotChange {
            tick: Tick::new(20),
            slot,
            kind: SlotChangeKind::Joined { from: Taken::Own },
        },
    ];
    assert_eq!(runner.log().changes(), changes);

    // The file decodes to a log that encodes to the same bytes, and replays to the same hashes.
    let bytes = encoded(runner.log());
    let decoded = SessionLog::decode(&bytes).unwrap();
    assert_eq!(encoded(&decoded), bytes);
    let mut replay = Replay::new(decoded, &store()).unwrap();
    let mut replayed = HashTrail::default();
    while replay.run_tick().unwrap() {
        replayed.record(replay.runner().world());
    }
    live.assert_same(&replayed);
    assert_eq!(replay.runner().log().changes(), changes);

    // A log whose server sent the leaver's slot to a bot breaks the mode's rules: the verifier
    // does not start it.
    let mut log = session.log();
    let to_bot = leave(AfterLeave::Bot);
    let signature = FixedSession::server_signature(&to_bot, log.session_id(), log.next_place());
    log.record_server(to_bot, &signature).unwrap();
    log.reveal_seed(FixedSession::seed(0));
    let refused = Replay::new(log, &store()).err();
    assert!(
        matches!(
            refused,
            Some(StartError::SlotRule {
                change: SlotChange { tick, .. },
                error: SlotRuleError::Leaver { becomes: AfterLeave::Bot },
            }) if tick == Tick::new(0)
        ),
        "{refused:?}"
    );
}

#[test]
fn a_3v3_log_with_learn_orders_verifies_from_the_store() {
    // The scripted 3v3 to tick 1900, Rime's learn, the last of its learn orders, checkpointed
    // before tick 40, and again without the checkpoint.
    let reference = Reference3v3::load();
    let mut snapshot = Vec::new();
    let play = |checkpoint: bool, snapshot: &mut Vec<u8>| {
        let mut fixed = reference.start();
        let mut trail = HashTrail::default();
        for tick in 0..=1900 {
            if checkpoint && tick == 40 {
                fixed.checkpoint(snapshot);
            }
            reference.play_tick(&mut fixed, tick);
            trail.record(fixed.runner().world());
        }
        fixed.end();
        (fixed, trail)
    };
    let (fixed, live) = play(true, &mut snapshot);
    let decoded = SessionLog::decode(&encoded(fixed.runner().log())).unwrap();
    let mut replay = Replay::new(decoded, &store()).unwrap();
    let record = replay.runner().log().checkpoint_at(Tick::new(40)).unwrap();
    assert_eq!(replay.check_snapshot(record, &snapshot), Ok(()));
    let mut replayed = HashTrail::default();
    while replay.run_tick().unwrap() {
        replayed.record(replay.runner().world());
    }
    live.assert_same(&replayed);
    // From tick 40 the match draws from segment 1's seed: its crits land otherwise than the
    // same match's without the checkpoint, which draws from segment 0's to the end.
    let (_, unbroken) = play(false, &mut Vec::new());
    let (live, unbroken) = (live.totals(), unbroken.totals());
    let first_difference = live.iter().zip(unbroken).position(|(a, b)| a != b);
    assert!(
        first_difference.is_some_and(|tick| tick >= 40),
        "{first_difference:?}"
    );
    // The replay learned what the match did: Cinder's and Veil's first basic ability, and Rime's
    // second.
    let world = replay.runner().world();
    let units = MatchUnits::of_world(world);
    let ranks = [(0, 0), (5, 0), (4, 1)].map(|(slot, ability)| {
        let entity = world
            .resource::<EntityIndex>()
            .get(units.hero(slot))
            .unwrap();
        let slots = world.get::<ActionSlots>(entity).unwrap();
        slots.slot(ability).unwrap().rank
    });
    assert_eq!(ranks, [1, 1, 1]);
}

/// The hero walks 1 m along x from its spawn, (0, −2), into the reach of the east tower.
const INTO_REACH: Sent = Sent {
    arrives: 0,
    stamp: 0,
    x: 1,
    z: -2,
    applied: Applied::At(Tick::new(0)),
};

/// What a tick left: the attack target of the towers, units 0 and 1, and of the creeps, 3 to 6;
/// the hero's health; and how many projectiles fly.
#[derive(Debug, PartialEq, Eq)]
struct Seen {
    targets: [Option<u64>; 6],
    hero_health: i64,
    projectiles: usize,
}

impl Seen {
    fn of(runner: &Runner) -> Seen {
        let world = runner.world();
        let index = world.resource::<EntityIndex>();
        let entity = |id: u64| index.iter().find(|(unit, _)| unit.get() == id).unwrap().1;
        let targets = [0, 1, 3, 4, 5, 6].map(|id| {
            let slots = world.entity(entity(id)).get::<ActionSlots>().unwrap();
            slots.attack_target().map(StableId::get)
        });
        let hero = world.entity(entity(2)).get::<Pools>().unwrap();
        let projectiles = index
            .iter()
            .filter(|&(_, entity)| world.entity(entity).contains::<Projectile>())
            .count();
        Seen {
            targets,
            hero_health: hero.current(LIFE).unwrap().round(),
            projectiles,
        }
    }
}

#[test]
fn scripted_creeps_and_towers_replay_to_the_same_hashes() {
    let Run { fixed, trail } = run(&[&INTO_REACH], 72);
    let decoded = SessionLog::decode(&encoded(fixed.runner().log())).unwrap();
    let mut replay = Replay::new(decoded, &store()).unwrap();
    let mut replayed = HashTrail::default();
    let mut seen = Vec::new();
    while replay.run_tick().unwrap() {
        replayed.record(replay.runner().world());
        seen.push(Seen::of(replay.runner()));
    }
    trail.assert_same(&replayed);

    // Ids: the map's towers, 0 at (−8, −3) on the west team and 1 at (8, −3) on the east, then the
    // hero 2, which the mode spawns as the match starts, then the first wave, at the end of tick
    // 0: the west creeps 3 and 4 at x = −16, and the east's 5 and 6 at x = 16. Every unit thinks
    // every 8 ticks, in the ticks that leave its id. Bodies: the towers 0.9 m, the hero 0.5 m, a
    // creep 0.35 m; a range counts from the edge of each body.
    //
    // The hero walks ¼ m a tick from (0, −2) in tick 0 and stands at (1, −2) from tick 3. Tower 1
    // takes the nearest hero that 6.25 m from the edge of its body reaches, 6.25 + 0.9 + 0.5 =
    // 7.65 m between centres: in tick 1 the hero at 0.25 is √(7.75² + 1) ≈ 7.81 m away, beyond; in
    // tick 9, at 1, √(7² + 1) ≈ 7.07 m, within: no creep is in reach, so it takes the hero. Its
    // weapon's reach is the same 7.65 m, so its attacks start in ticks 9 and 46, and fire 5 ticks
    // later, from (8, −3), at the hero 7.07 m away. A projectile of no width flies 12 m/s ÷ 30,
    // 0.4 m rounded down to 6710886 / 2²⁴ m, from the tick after it fires, and hits when it comes
    // within the hero's 0.5 m: 16 steps leave 7.07 − 6.4 ≈ 0.67 m, 17 leave 0.27 m, so the 17th
    // hits, in ticks 31 and 68, for 150 each.
    //
    // The east creeps walk ⅛ m a tick from tick 1, where their bodies, both at 15.875, part to
    // 15.525 and 16.225; each thinks in tick t where the tick before left it, and takes the hero
    // that its 6 m aggro range reaches from its edge, 6 + 0.35 + 0.5 = 6.85 m between centres.
    // Creep 5 thinks in tick 69 at 15.525 − 67/8 ≈ 7.15, √(6.15² + 2²) ≈ 6.47 m from the hero,
    // within, and in tick 61 at 8.15, √(7.15² + 4) ≈ 7.42 m, beyond: it takes the hero in tick 69.
    // Creep 6 thinks in tick 70 at 16.225 − 68/8 = 7.725, √(6.725² + 4) ≈ 7.02 m, beyond: it takes
    // no one. The west creeps are 15 m from the east's, and tower 0 is 15.5 m from them: none of
    // them takes a target.
    let expected: Vec<_> = (0..72)
        .map(|tick| {
            let hero = Some(2);
            let from = |first: u64| (tick >= first).then_some(()).and(hero);
            let hero_health = match tick {
                ..31 => 600,
                31..68 => 450,
                _ => 300,
            };
            let projectiles = usize::from((14..31).contains(&tick) || (51..68).contains(&tick));
            Seen {
                targets: [None, from(9), None, None, from(69), None],
                hero_health,
                projectiles,
            }
        })
        .collect();
    assert_eq!(seen, expected);
}

fn encoded(log: &SessionLog) -> Vec<u8> {
    let mut bytes = Vec::new();
    log.encode(&mut bytes);
    bytes
}

#[test]
fn the_binary_logs_the_last_state_hash() {
    let mut snapshot = Vec::new();
    let Run { fixed, trail } = checkpointed(&mut snapshot, None, None);
    let runner = fixed.runner();
    // A directory of this run's own, which goes when the test ends, passed or failed.
    let scratch = TempDir::new_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let dir = scratch.path().to_str().unwrap();
    let path = format!("{dir}/headless.log");
    fs::write(&path, encoded(runner.log())).unwrap();
    // Logged without color, as standard error is not a terminal here, at `info`, and to no file,
    // whatever the environment says.
    let verifier = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_campfire-verifier"))
            .args(args)
            .env("RUST_LOG", "info")
            .env_remove("CAMPFIRE_LOG")
            .env_remove("CAMPFIRE_LOG_FILTER")
            .output()
            .unwrap()
    };
    // Every package, the reference ones and the test ones: what the verifier holds.
    let packages = PackageDir::workspace("");
    let packages = packages.to_str().unwrap();
    let hash = trail.totals().last().unwrap();
    let success = format!("{} file={path} hash={hash}\n", Verified::MESSAGE);
    // With no snapshot, and with the checkpoint's snapshot named by its fingerprint.
    let snapshots = format!("{dir}/snapshots");
    fs::create_dir(&snapshots).unwrap();
    let named = format!("{snapshots}/{}", SnapshotFingerprint::of(&snapshot));
    fs::write(&named, &snapshot).unwrap();
    for args in [&[packages, &path][..], &[packages, &path, &snapshots]] {
        let output = verifier(args);
        assert!(output.status.success(), "{output:?}");
        let logged = String::from_utf8(output.stderr).unwrap();
        assert!(logged.ends_with(&success), "{logged}");
        assert!(output.stdout.is_empty());
    }
    // A snapshot of other bytes under the checkpoint's name.
    *snapshot.last_mut().unwrap() ^= 1;
    fs::write(&named, &snapshot).unwrap();
    let output = verifier(&[packages, &path, &snapshots]);
    assert_eq!(output.status.code(), Some(1));
    let logged = String::from_utf8(output.stderr).unwrap();
    let refused = format!(
        "the log does not verify file={path} error=the snapshot is not the one its checkpoint \
         names\n"
    );
    assert!(logged.ends_with(&refused), "{logged}");

    let corrupt = format!("{dir}/headless-truncated.log");
    let bytes = encoded(runner.log());
    fs::write(&corrupt, &bytes[..bytes.len() - 1]).unwrap();
    let output = verifier(&[packages, &corrupt]);
    assert_eq!(output.status.code(), Some(1));
    let logged = String::from_utf8(output.stderr).unwrap();
    let refused =
        format!("the log does not verify file={corrupt} error=session log ends inside a field\n");
    assert!(logged.ends_with(&refused), "{logged}");

    assert_eq!(verifier(&[]).status.code(), Some(2));
}

#[test]
fn a_log_replays_only_with_its_seed_its_release_and_its_packages() {
    let _log = LogCheck::start();
    let store = store();
    let session = session();
    assert!(matches!(
        Replay::new(session.log(), &store),
        Err(StartError::Seed(SeedError::NotRevealed))
    ));
    assert!(matches!(
        Runner::new(
            session.log(),
            ServerSeeds::new(0, ServerSeed::new([8; 32])),
            &packages()
        ),
        Err(StartError::Seed(SeedError::WrongSeed))
    ));

    // Each change to the terms, the log revealed, and the error the verifier refuses it with.
    let other = |change: fn(&mut SessionTerms)| {
        let mut terms = session.terms().clone();
        change(&mut terms);
        let mut log = SessionLog::new(session.header(terms)).unwrap();
        log.reveal_seed(FixedSession::seed(0));
        Replay::new(log, &store).err()
    };
    let refused = [
        other(|terms| terms.release = "0.0.9".to_owned()),
        // Another release first: its packages need not read in this one.
        other(|terms| {
            terms.release = "0.0.9".to_owned();
            terms.mode = Fingerprint::new([0; 32]);
        }),
        other(|terms| terms.mode = Fingerprint::new([0; 32])),
        other(|terms| terms.dependencies[0] = Fingerprint::new([0; 32])),
        other(|terms| terms.dependencies.clear()),
        other(|terms| terms.tick_hz = NonZeroU32::new(60).unwrap()),
    ];
    assert!(
        matches!(
            &refused,
            [
                Some(StartError::Terms(TermsError::OtherRelease(release))),
                Some(StartError::Terms(TermsError::OtherRelease(_))),
                Some(StartError::Packages(StoreError::UnknownMode)),
                Some(StartError::Packages(StoreError::MissingDependency(dependency))),
                Some(StartError::Packages(StoreError::DependencyCount)),
                Some(StartError::Terms(TermsError::TickRate(hz))),
            ] if release == "0.0.9" && dependency == "hero-runner" && hz.get() == 60
        ),
        "{refused:?}"
    );

    // Three slots, the session's player and two open, and the lane's two teams seat one each.
    let mut terms = session.terms().clone();
    terms.slots.extend([SlotPlan::Open; 2]);
    let mut log = SessionLog::new(session.header(terms)).unwrap();
    log.reveal_seed(FixedSession::seed(0));
    let refused = Replay::new(log, &store).err();
    assert!(
        matches!(
            refused,
            Some(StartError::Terms(TermsError::Slots { slots: 3, most: 2 }))
        ),
        "{refused:?}"
    );

    // A lane mode whose `on_match_start` throws: the session does not start.
    let mut files = files_under(&PackageDir::workspace("test"));
    let script = PathBuf::from("modes/lane/scripts/mode.rhai");
    let text = String::from_utf8(files[&script].clone()).unwrap();
    let start = "fn on_match_start(ctx) {\n";
    assert!(text.contains(start));
    let text = text.replacen(
        start,
        "fn on_match_start(ctx) {\n    throw \"no start\";\n",
        1,
    );
    files.insert(script, text.into_bytes());
    let dir = PackageDir::in_memory(Arc::new(files), "modes/lane");
    let thrown = FixedSession::new(
        ModePackages::from_package_dir(&dir).unwrap(),
        NonZeroU32::new(30).unwrap(),
        1,
    );
    let refused = Runner::new(thrown.log(), FixedSession::seeds(), thrown.packages()).err();
    let Some(refused @ StartError::MatchStart(_)) = refused else {
        panic!("{refused:?}");
    };
    assert_eq!(
        refused.to_string(),
        r#"the mode's start failed: script call raised "no start""#
    );
}
