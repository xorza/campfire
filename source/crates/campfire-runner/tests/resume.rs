//! A match resumed from a checkpoint, its state the checkpoint's snapshot and its log replayed
//! to the checkpoint's tick, plays each later tick to the hash of the run that made it: every
//! value a tick reads is state, or derived data a restore builds again.

use std::num::NonZeroU32;

use campfire_capabilities::{Dead, Deaths};
use campfire_common::{MapName, StateHash, Tick};
use campfire_log::ErrorReport;
use campfire_package::{ModePackages, PackageDir};
use campfire_protocol::{Checkpoint, Outcome, SessionLog, SnapshotFingerprint};
use campfire_runner::internals::{FixedMatch, FixedSession, Moba3v3, ProvingMatch};
use campfire_runner::{ResumeError, Runner};
use campfire_sim::SnapshotError;
use campfire_sim::{EntityIndex, StableId};

/// A run of a match with a checkpoint before each of three ticks: its log, the snapshot of each
/// checkpoint, the state hash after each tick, and the ticks a unit died and came back in.
#[derive(Debug)]
struct Checkpointed {
    log: Vec<u8>,
    snapshots: Vec<Vec<u8>>,
    hashes: Vec<StateHash>,
    deaths: Vec<u64>,
    respawns: Vec<u64>,
}

impl Checkpointed {
    /// Plays `fixed` for `ticks` ticks, each as `play` plays it, with a checkpoint before each
    /// tick of `at`.
    fn run(
        mut fixed: FixedMatch,
        at: [u64; 3],
        ticks: u64,
        mut play: impl FnMut(&mut FixedMatch, u64),
    ) -> Checkpointed {
        let mut snapshots = Vec::new();
        let mut hashes = Vec::new();
        let (mut deaths, mut respawns) = (Vec::new(), Vec::new());
        let mut dead: Vec<StableId> = Vec::new();
        for tick in 0..ticks {
            if at.contains(&tick) {
                let mut snapshot = Vec::new();
                fixed.checkpoint(&mut snapshot);
                snapshots.push(snapshot);
            }
            play(&mut fixed, tick);
            let world = fixed.runner().world();
            hashes.push(fixed.runner().state_hash());
            if world
                .get_resource::<Deaths>()
                .is_some_and(|deaths| deaths.tick().get() == tick && deaths.iter().next().is_some())
            {
                deaths.push(tick);
            }
            let now: Vec<StableId> = world
                .resource::<EntityIndex>()
                .iter()
                .filter(|&(_, entity)| world.entity(entity).contains::<Dead>())
                .map(|(id, _)| id)
                .collect();
            let index = world.resource::<EntityIndex>();
            if dead
                .iter()
                .any(|&id| index.get(id).is_some() && !now.contains(&id))
            {
                respawns.push(tick);
            }
            dead = now;
        }
        let mut log = Vec::new();
        fixed.runner().log().encode(&mut log);
        Checkpointed {
            log,
            snapshots,
            hashes,
            deaths,
            respawns,
        }
    }

    /// Resumes the match of `packages` from each checkpoint, and checks that it plays each tick
    /// up to the next checkpoint, or the run's end, to the run's hash; and that a death and a
    /// respawn came between the first checkpoint and the last.
    fn check(&self, packages: &ModePackages, at: [u64; 3]) {
        let between = |ticks: &[u64]| ticks.iter().any(|tick| (at[0]..at[2]).contains(tick));
        assert!(
            between(&self.deaths) && between(&self.respawns),
            "deaths in {:?}, respawns in {:?}",
            self.deaths,
            self.respawns
        );
        let ticks = u64::try_from(self.hashes.len()).unwrap();
        for (checkpoint, snapshot) in (0..).zip(&self.snapshots) {
            let log = SessionLog::decode(&self.log).unwrap().rewound();
            let segment = checkpoint + 1;
            let mut runner =
                Runner::resume(log, FixedSession::seeds(), packages, segment, snapshot)
                    .unwrap_or_else(|error| {
                        panic!("segment {segment}: {}", ErrorReport::of(&error))
                    });
            let start = at[checkpoint as usize];
            let end = at.get(checkpoint as usize + 1).copied().unwrap_or(ticks);
            for tick in start..end {
                runner.run_tick();
                assert_eq!(
                    runner.state_hash(),
                    self.hashes[usize::try_from(tick).unwrap()],
                    "resumed before tick {start}, tick {tick}"
                );
            }
        }
    }
}

#[test]
fn the_proving_match_resumes_from_each_checkpoint_to_the_same_hashes() {
    let proving = ProvingMatch::load();
    let at = [100, 250, 400];
    let run = Checkpointed::run(
        proving.start(),
        at,
        ProvingMatch::TICKS,
        ProvingMatch::play_tick,
    );
    run.check(proving.packages(), at);
}

#[test]
fn the_lane_match_resumes_from_each_checkpoint_to_the_same_hashes() {
    let packages = ModePackages::from_dir(
        &PackageDir::workspace("test/modes/lane"),
        &MapName::new("lane").unwrap(),
    )
    .unwrap();
    let session = FixedSession::new(packages, NonZeroU32::new(30).unwrap(), 2);
    let at = [100, 300, 600];
    let run = Checkpointed::run(session.start(), at, 900, |fixed, _| {
        fixed.runner_mut().run_tick();
    });
    run.check(session.packages(), at);
}

#[test]
fn the_3v3_match_resumes_from_each_checkpoint_to_the_same_hashes() {
    let moba = Moba3v3::load();
    let at = [1000, 2000, 3000];
    let run = Checkpointed::run(moba.start(), at, 4000, |fixed, tick| {
        moba.play_tick(fixed, tick);
    });
    run.check(moba.packages(), at);
}

#[test]
fn a_resume_refuses_a_checkpoint_its_log_or_snapshot_does_not_give() {
    // Two checkpoints the server signed with flaws: the first, before tick 10, names a state hash
    // its snapshot does not restore to; the second, before tick 20, fingerprints bytes that are
    // no snapshot.
    let proving = ProvingMatch::load();
    let mut fixed = proving.start();
    let session_id = fixed.runner().log().session_id();
    let checkpoint = |fixed: &mut FixedMatch, flaw: fn(&mut Checkpoint, &mut Vec<u8>)| {
        let mut snapshot = Vec::new();
        fixed.runner_mut().begin_checkpoint().unwrap();
        let mut record = fixed.runner().checkpoint(&mut snapshot).unwrap();
        flaw(&mut record, &mut snapshot);
        let signature = FixedSession::checkpoint_signature(&record, session_id);
        fixed
            .runner_mut()
            .record_checkpoint(record, &signature)
            .unwrap();
        snapshot
    };
    let mut first = Vec::new();
    for tick in 0..20 {
        if tick == 10 {
            first = checkpoint(&mut fixed, |record, _| {
                record.state_hash = StateHash::new([0; 32]);
            });
        }
        ProvingMatch::play_tick(&mut fixed, tick);
    }
    let garbage = checkpoint(&mut fixed, |record, snapshot| {
        *snapshot = b"no snapshot".to_vec();
        record.snapshot = SnapshotFingerprint::of(snapshot);
    });
    let mut bytes = Vec::new();
    fixed.runner().log().encode(&mut bytes);
    let resume = |segment, snapshot: &[u8]| {
        let log = SessionLog::decode(&bytes).unwrap().rewound();
        Runner::resume(
            log,
            FixedSession::seeds(),
            proving.packages(),
            segment,
            snapshot,
        )
        .err()
    };
    assert!(matches!(resume(3, &first), Some(ResumeError::NoCheckpoint)));
    assert!(matches!(
        resume(1, &garbage),
        Some(ResumeError::Fingerprint)
    ));
    assert!(matches!(resume(1, &first), Some(ResumeError::StateHash)));
    assert!(matches!(
        resume(2, &garbage),
        Some(ResumeError::Snapshot(SnapshotError::NotSnapshot))
    ));
}

#[test]
fn a_load_drops_the_log_after_its_save_and_the_published_log_verifies() {
    // A save before tick 100, segment 1, and a checkpoint before tick 150, segment 2.
    let proving = ProvingMatch::load();
    let mut fixed = proving.start();
    let id = fixed.runner().log().session_id();
    let mut hashes = Vec::new();
    let mut save = Vec::new();
    for tick in 0..200 {
        if tick == 100 {
            fixed.checkpoint(&mut save);
        }
        if tick == 150 {
            fixed.checkpoint(&mut Vec::new());
        }
        ProvingMatch::play_tick(&mut fixed, tick);
        hashes.push(fixed.runner().state_hash());
    }
    let record = fixed
        .runner()
        .log()
        .checkpoint_at(Tick::new(100))
        .unwrap()
        .clone();

    // The load of the save: the match stands before tick 100 again, and plays 50 ticks with no
    // order, unlike the ticks the load dropped.
    let mut runner = fixed
        .into_runner()
        .load(1, &save, proving.packages())
        .unwrap();
    assert_eq!(runner.log().next_tick(), Tick::new(100));
    assert_eq!(runner.state_hash(), hashes[99]);
    for _ in 100..150 {
        runner.run_tick();
    }
    assert_ne!(runner.state_hash(), hashes[149]);
    let result = runner.result_as(Outcome::Aborted);
    let signature = FixedSession::result_signature(&result, id);
    runner.record_result(result, &signature).unwrap();
    runner.reveal_seed();
    let mut bytes = Vec::new();
    runner.log().encode(&mut bytes);

    // The published log holds the segments up to the save, and the one the load started: its
    // replay meets the save's hash and the result.
    let published = SessionLog::decode(&bytes).unwrap();
    assert_eq!(published.next_tick(), Tick::new(150));
    assert_eq!(published.checkpoints().collect::<Vec<_>>(), [&record]);
    let seeds = published.revealed_seeds().unwrap();
    let mut replay = Runner::new(published.rewound(), seeds, proving.packages()).unwrap();
    while replay.log().next_tick() < Tick::new(150) {
        if replay.log().next_tick() == Tick::new(100) {
            assert_eq!(replay.state_hash(), record.state_hash);
        }
        replay.run_tick();
    }
    assert_eq!(replay.check_result(), Ok(()));
}
