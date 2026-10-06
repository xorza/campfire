use std::path::{Path, PathBuf};

use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_common::{Tick, Ticks};
use campfire_log::LogEvent;
use campfire_protocol::{CheckpointBegun, CheckpointError, DurableError, Outcome};
use campfire_runner::{CheckpointBeginError, Session};
use campfire_sim::StateDelta;

use crate::checkpoint_thread::{CheckpointThread, Returned, SignedCheckpoint};
use crate::events::checkpoint_taken::CheckpointTaken;
use crate::events::seeds_ran_out::SeedsRanOut;
use crate::server_signer::ServerSigner;
use crate::sim_server::SimServer;

/// How often, in ticks, the main thread sends the state changed to the checkpoint thread when no
/// checkpoint is due, so what the match records for the next delta stays as small as this many
/// ticks of changes, and no delta copies more.
const SEND_EVERY: Ticks = Ticks::new(100);

/// A session's checkpoints on its server: the boundaries it checkpoints at, and the thread that
/// takes them. At a boundary due, right after the tick before it, the main thread begins the
/// checkpoint, which starts its segment, and sends the state changed since the last delta to the
/// thread; once the thread wrote the snapshot and signed the record, the main thread logs the
/// record. A checkpoint past the seed chain's last segment ends the session aborted.
#[derive(Resource, Debug)]
pub(crate) struct Checkpoints {
    /// The ticks whose boundaries a checkpoint is due at, ascending.
    plan: Vec<Tick>,
    thread: CheckpointThread,
    /// The boundary the last delta was made at.
    sent: Tick,
    /// The failure of a snapshot's write, once one failed.
    failure: Option<DurableError>,
}

impl Checkpoints {
    /// Starts the checkpoints of the session in `world`, before the next tick runs: the thread's
    /// copy of the state starts as a full copy of the state now, and the snapshots go to
    /// `snapshots`.
    pub(crate) fn start(world: &mut World, snapshots: PathBuf, signer: ServerSigner) {
        let mut base = StateDelta::default();
        Session::track(world, &mut base);
        let session = world.resource::<Session>();
        let sent = session.log().next_tick();
        let thread = CheckpointThread::start(
            session.registry().clone(),
            base,
            session.log().session_id(),
            snapshots,
            signer,
        );
        world.insert_resource(Checkpoints {
            plan: Vec::new(),
            thread,
            sent,
            failure: None,
        });
    }

    /// Makes a checkpoint due at the boundary before `tick`.
    pub(crate) fn request(&mut self, tick: Tick) {
        if let Err(at) = self.plan.binary_search(&tick) {
            self.plan.insert(at, tick);
        }
    }

    /// At the boundary before the next tick: begins the checkpoint due there, once the last delta
    /// came back, past the seed chain's last segment ending the session aborted instead; with
    /// none due, sends the state changed when `SEND_EVERY` ticks passed since the last delta and
    /// it came back.
    pub(crate) fn begin(world: &mut World) {
        let next = world.resource::<Session>().log().next_tick();
        let mut checkpoints = world.resource_mut::<Checkpoints>();
        let due = checkpoints.plan.first() == Some(&next);
        checkpoints.plan.retain(|&tick| tick > next);
        if !due {
            if checkpoints.sent.after(SEND_EVERY) <= next && !checkpoints.thread.pending() {
                Checkpoints::send(world, None);
            }
            return;
        }
        if let Err(error) = Checkpoints::settle(world) {
            world.resource_mut::<Checkpoints>().failure = Some(error);
            return;
        }
        match world.resource_mut::<Session>().begin_checkpoint() {
            Ok(begun) => Checkpoints::send(world, Some(begun)),
            Err(CheckpointBeginError::PastSeeds) => {
                SeedsRanOut { tick: next }.log();
                SimServer::end_session(world, Outcome::Aborted)
                    .expect("no checkpoint is on the thread");
            }
            // A checkpoint begun at this boundary already, as a restore takes one again.
            Err(CheckpointBeginError::Log(CheckpointError::Empty)) => {}
            Err(error) => panic!("the session's own checkpoint: {error}"),
        }
    }

    /// Logs the record of the checkpoint the thread finished, when it did.
    pub(crate) fn finish(world: &mut World) {
        let returned = world.resource_mut::<Checkpoints>().thread.done(false);
        Checkpoints::record(world, returned);
    }

    /// Waits for the delta on the thread, and logs the record of its checkpoint; an error when
    /// its snapshot was not written.
    pub(crate) fn settle(world: &mut World) -> Result<(), DurableError> {
        let returned = world.resource_mut::<Checkpoints>().thread.done(true);
        Checkpoints::record(world, returned);
        match world.resource_mut::<Checkpoints>().failure.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// The failure of a snapshot's write, once one failed, given once.
    pub(crate) fn take_failure(&mut self) -> Option<DurableError> {
        self.failure.take()
    }

    /// Takes the checkpoint begun at the boundary before the next tick again, on the main thread,
    /// as a restore does once its replay reaches it: writes its snapshot into `snapshots`, and
    /// logs its record, which `signer` signs. Nothing when no checkpoint begun starts there.
    pub(crate) fn take_again(
        world: &mut World,
        snapshots: &Path,
        signer: &ServerSigner,
    ) -> Result<(), DurableError> {
        let session = world.resource::<Session>();
        let next = session.log().next_tick();
        if session
            .log()
            .begun_checkpoint()
            .is_none_or(|begun| begun.tick != next)
        {
            return Ok(());
        }
        let mut snapshot = Vec::new();
        let record = session
            .checkpoint(world, &mut snapshot)
            .expect("a checkpoint begun");
        CheckpointThread::write_snapshot(snapshots, &snapshot)?;
        let signature = signer.sign_checkpoint(&record, session.log().session_id());
        Checkpoints::log(world, SignedCheckpoint { record, signature });
        Ok(())
    }

    /// Sends the thread the state changed since the last delta, with the checkpoint `begun` at
    /// the boundary before the next tick, when one is.
    fn send(world: &mut World, begun: Option<CheckpointBegun>) {
        let next = world.resource::<Session>().log().next_tick();
        world.resource_scope(|world, mut checkpoints: Mut<'_, Checkpoints>| {
            checkpoints
                .thread
                .send(begun, |delta| Session::changes(world, delta));
            checkpoints.sent = next;
        });
    }

    fn record(world: &mut World, returned: Option<Returned>) {
        match returned {
            None | Some(Returned::Applied) => {}
            Some(Returned::Taken(signed)) => Checkpoints::log(world, signed),
            Some(Returned::Failed(error)) => {
                world.resource_mut::<Checkpoints>().failure = Some(error);
            }
        }
    }

    /// Logs the record `signed` brings.
    fn log(world: &mut World, signed: SignedCheckpoint) {
        let SignedCheckpoint { record, signature } = signed;
        let taken = CheckpointTaken {
            segment: record.segment,
            tick: record.tick,
        };
        world
            .resource_mut::<Session>()
            .record_checkpoint(record, &signature)
            .expect("the server's own checkpoint holds");
        taken.log();
    }
}
