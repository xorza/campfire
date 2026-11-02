mod auras;
mod bots;
mod checkpoints;
mod console;
mod deaths;
mod fog;
mod leak;
mod pace;
mod prototype;
mod receipts;
mod rejoin;
mod restore;
mod riding;
mod saves;
mod scenario;
mod short_3v3;
mod spectators;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bevy_app::App;
use bevy_ecs::query::Allow;
use campfire_common::{PlayerSlot, StateHash};
use campfire_harness::MatchUnits;
use campfire_host::{ServerDir, SessionDir};
use campfire_net::SimServerWorld;
use campfire_net_harness::InProcessMatch;
use campfire_protocol::SessionLog;
use campfire_runner::{Replay, Session, SessionWorld};
use campfire_sim::{StableId, StateRegistry, Unpredicted};
use campfire_store::InputFile;
use campfire_store::internals::TestDir;
use campfire_units::Destination;

/// A match a test played, and the state hash after each tick its server ran.
#[derive(Debug)]
struct PlayedMatch {
    game: InProcessMatch,
    hashes: Vec<StateHash>,
}

/// The directory the snapshots of `local`'s session go to, its server's data in `test_dir`'s
/// `data`.
fn snapshots(test_dir: &TestDir, local: &InProcessMatch) -> PathBuf {
    let id = session_log(local).session_id();
    test_dir
        .path("data/sessions")
        .join(id.to_string())
        .join("snapshots")
}

/// Ends `local`'s session, publishes its log, and replays it as a verifier does with the
/// snapshots in `snapshots`: each is the one its checkpoint fingerprints and restores to its
/// state hash; the replay, at its start, to run as the test asks.
fn published_replay<'p>(local: &'p mut InProcessMatch, snapshots: &Path) -> Replay<'p> {
    local.server_mut().world_mut().end_session().unwrap();
    let shape = local
        .server_mut()
        .world_mut()
        .session_mut()
        .close_journal()
        .unwrap();
    let held = local.server().world().resource::<Session>().log();
    let revealed = held.revealed_seeds().unwrap();
    let last = revealed.seed(held.segment()).unwrap().clone();
    let id = held.session_id();
    let data = local.server().world().resource::<ServerDir>();
    let file = SessionDir::publish(data, id, &shape, &last).unwrap();
    let bytes = InputFile::read(&file, SessionLog::MAX_FILE_LEN).unwrap();
    let published = SessionLog::decode(&bytes).unwrap();
    let mut replay = Replay::of(published, local.packages()).unwrap();
    let records: Vec<_> = (replay.game().session().log().checkpoints())
        .cloned()
        .collect();
    for record in &records {
        let file = snapshots.join(record.snapshot.to_string());
        let snapshot = InputFile::read(&file, StateRegistry::MAX_SNAPSHOT_LEN).unwrap();
        assert_eq!(replay.check_snapshot(record, &snapshot), Ok(()));
    }
    replay
}

/// The server's session log of `local`.
fn session_log(local: &InProcessMatch) -> &SessionLog {
    local.server().world().resource::<Session>().log()
}

/// Where the server's hero of `player_slot` walks to.
fn destination(local: &InProcessMatch, player_slot: PlayerSlot) -> Destination {
    let units = MatchUnits::of(local.server().world());
    let hero = units.hero(player_slot.get());
    *units.unit(hero).get::<Destination>().unwrap()
}

/// The stable ids of the entities `app` holds, predicted or not.
fn held(app: &mut App) -> BTreeSet<StableId> {
    let world = app.world_mut();
    let mut ids = world.query_filtered::<&StableId, Allow<Unpredicted>>();
    ids.iter(world).copied().collect()
}
