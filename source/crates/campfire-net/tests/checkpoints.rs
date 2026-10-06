//! Checkpoints on a server that keeps its data: taken on their thread at the boundaries due, a
//! restore builds the match from the latest one with a record and replays from there, takes a
//! checkpoint begun with no record again, and the published log verifies with its snapshots; a
//! checkpoint past the seed chain's last segment ends the session aborted.

use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use campfire_common::{StateHash, Tick};
use campfire_net::internals::{End, LinkModel, LocalMatch, MatchSetup};
use campfire_net::{SeedsRanOut, SessionDir, SimServer, TickHashes};
use campfire_package::ModePackages;
use campfire_protocol::{JournalFrames, Outcome, SeedChain, SessionLog, SnapshotFingerprint};
use campfire_runner::{Runner, Session};

use crate::Scratch;

/// The chain of the scenario's sessions, of room for three checkpoints.
const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::new(4).unwrap());

/// The scenario's match, its server's data in `data`, checkpointed before ticks 30 and 160, after
/// 180 steps with both records logged; with the state hash after each tick the server ran. The
/// second checkpoint's delta holds the changes from tick 130 on: the server sent those before
/// it, 100 ticks after the first.
fn checkpointed(data: &Scratch) -> (LocalMatch, Vec<StateHash>) {
    let mut local = LocalMatch::new(MatchSetup::duo(LinkModel::PERFECT, SEED_CHAIN));
    local.keep_data(data.0.clone());
    local.start_match();
    local.play_by_team(LocalMatch::SCENARIO_SCRIPTS);
    for tick in [30, 160] {
        SimServer::request_checkpoint(local.server_mut().world_mut(), Tick::new(tick));
    }
    // The steps outrun the disk: each waits for its checkpoint's snapshot to be written.
    for steps in [40, 140] {
        for _ in 0..steps {
            local.step();
        }
        SimServer::settle_checkpoint(local.server_mut().world_mut()).unwrap();
    }
    assert_eq!(log(&local).checkpoints().count(), 2);
    let hashes = local
        .server()
        .world()
        .resource::<TickHashes>()
        .get()
        .to_vec();
    (local, hashes)
}

fn log(local: &LocalMatch) -> &SessionLog {
    local.server().world().resource::<Session>().log()
}

/// The directory the session's snapshots go to.
fn snapshots(data: &Scratch, local: &LocalMatch) -> PathBuf {
    let id = log(local).session_id();
    data.0
        .join("sessions")
        .join(id.to_string())
        .join("snapshots")
}

/// Ends the session aborted, publishes its log, and checks it as a verifier does with the
/// snapshots in `snapshots`: each snapshot is the one its checkpoint fingerprints and restores to
/// its state hash, and the replay from tick 0 meets each checkpoint's hash and the result.
fn verifies(local: &mut LocalMatch, data: &Scratch, snapshots: &Path) {
    SimServer::end_session(local.server_mut().world_mut(), Outcome::Aborted).unwrap();
    let file = SessionDir::publish(&data.0, log(local)).unwrap();
    let published = SessionLog::decode(&fs::read(file).unwrap()).unwrap();
    let packages: &ModePackages = local.packages();
    for record in published.checkpoints() {
        let snapshot = fs::read(snapshots.join(record.snapshot.to_string())).unwrap();
        assert_eq!(SnapshotFingerprint::of(&snapshot), record.snapshot);
        let restored = Session::snapshot_hash(packages, published.header(), &snapshot).unwrap();
        assert_eq!(restored, record.state_hash);
    }
    let ticks = published.next_tick();
    let seeds = published.revealed_seeds().unwrap();
    let mut replay = Runner::new(published.rewound(), seeds, packages).unwrap();
    while replay.log().next_tick() < ticks {
        let next = replay.log().next_tick();
        if let Some(record) = replay.log().checkpoint_at(next) {
            assert_eq!(replay.state_hash(), record.state_hash, "tick {next}");
        }
        replay.run_tick();
    }
    assert_eq!(replay.check_result(), Ok(()));
}

#[test]
fn a_restart_resumes_from_the_latest_checkpoint_and_its_log_verifies_with_its_snapshots() {
    let data = Scratch::new("checkpoints");
    let (mut local, before) = checkpointed(&data);
    let cut = local.next_tick(End::Server);
    local.restart_server();
    // The restore builds the match from the snapshot before tick 160, and replays from there.
    let world = local.server().world();
    let at = usize::try_from(cut).unwrap();
    assert_eq!(world.resource::<TickHashes>().get(), &before[160..at]);
    assert_eq!(local.next_tick(End::Server), cut);
    for _ in 0..30 {
        local.step();
    }
    let snapshots = snapshots(&data, &local);
    verifies(&mut local, &data, &snapshots);
}

#[test]
fn a_checkpoint_cut_between_its_begin_and_its_record_is_taken_again() {
    let data = Scratch::new("checkpoint-cut");
    let (mut local, before) = checkpointed(&data);
    let second = log(&local).checkpoint_at(Tick::new(160)).unwrap().clone();
    let snapshots = snapshots(&data, &local);
    let id = log(&local).session_id();
    local.stop_server();

    // A crash between the second checkpoint's begin and its record: the journal ends before
    // the record, and the snapshot is gone.
    let journal = data.0.join("sessions").join(id.to_string()).join("journal");
    let bytes = fs::read(&journal).unwrap();
    let mut frames = JournalFrames::new(&bytes).unwrap();
    let mut done = Vec::new();
    loop {
        let start = frames.whole();
        let Some(record) = frames.next() else {
            break;
        };
        if record[0] == 4 {
            done.push(start);
        }
    }
    assert_eq!(done.len(), 2);
    fs::write(&journal, &bytes[..done[1]]).unwrap();
    let file = snapshots.join(second.snapshot.to_string());
    fs::remove_file(&file).unwrap();

    // The restore builds the match from the first checkpoint, replays to the cut, and takes the
    // second again as its replay passes tick 160: the same record, its snapshot written again.
    let dir = SessionDir::find(&data.0).unwrap().unwrap();
    let cut = dir.restore().unwrap().unwrap().log.next_tick();
    assert!(cut > Tick::new(160), "{cut}");
    local.restart_server();
    let world = local.server().world();
    let at = usize::try_from(cut.get()).unwrap();
    assert_eq!(world.resource::<TickHashes>().get(), &before[30..at]);
    let again = log(&local).checkpoint_at(Tick::new(160)).unwrap();
    assert_eq!(
        (again.state_hash, again.snapshot, &again.carry),
        (second.state_hash, second.snapshot, &second.carry)
    );
    assert!(file.exists());
    verifies(&mut local, &data, &snapshots);
}

#[test]
fn a_checkpoint_past_the_seed_chain_ends_the_session_aborted() {
    // A chain of one segment: the checkpoint before tick 30 would start a second.
    let data = Scratch::new("checkpoint-seeds");
    let mut local = LocalMatch::new(MatchSetup::duo(LinkModel::PERFECT, LocalMatch::SEED_CHAIN));
    local.keep_data(data.0.clone());
    local.start_match();
    SimServer::request_checkpoint(local.server_mut().world_mut(), Tick::new(30));
    for _ in 0..60 {
        local.step();
    }
    let result = *log(&local).result().unwrap();
    assert_eq!(
        (result.tick, result.outcome),
        (Tick::new(30), Outcome::Aborted)
    );
    assert_eq!(local.next_tick(End::Server), 30);
    assert_eq!(
        local.log().take::<SeedsRanOut>(),
        [SeedsRanOut {
            tick: Tick::new(30)
        }]
    );
}
