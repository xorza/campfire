use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_common::{Tick, Ticks};
use campfire_log::{ErrorReport, LogEvent};
use campfire_mode::SaveBy;
use campfire_protocol::{CheckpointBegun, CheckpointError, Outcome};
use campfire_runner::{CheckpointBeginError, Session, SessionWorld};
use campfire_sim::{SimWorld, StateDelta, StateRegistry};
use campfire_store::{DurableError, PathError};

use crate::events::checkpoint_taken::CheckpointTaken;
use crate::events::seeds_ran_out::SeedsRanOut;
use crate::faults::Faults;
use crate::faults::fault::Fault;
use crate::save_command::SaveCommand;
use crate::sim_server::checkpoint_thread::{CheckpointThread, Returned, SignedCheckpoint};
use crate::sim_server::checkpoints::error::SaveRefusal;
use crate::sim_server::local_session::LocalSession;
use crate::sim_server::server_signer::ServerSigner;
use crate::sim_server::snapshot_dir::SnapshotDir;

pub(crate) mod error;

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
}

/// The links that may send save commands, with whether each is refused.
impl Checkpoints {
    /// Starts the checkpoints of the session in `world`, before the next tick runs: the thread's
    /// copy of the state starts as a full copy of the state now, and the snapshots go to
    /// `snapshots`.
    pub(super) fn start(world: &mut World, snapshots: SnapshotDir, signer: ServerSigner) {
        let mut base = StateDelta::default();
        world.track(&mut base);
        let session = world.resource::<Session>();
        let thread = CheckpointThread::start(
            world.resource::<StateRegistry>().clone(),
            base,
            session.log().session_id(),
            snapshots,
            signer,
        );
        world.insert_resource(Checkpoints {
            plan: Vec::new(),
            thread,
        });
    }

    /// Makes a checkpoint due at the boundary before `tick`.
    pub(crate) fn request(&mut self, tick: Tick) {
        if let Err(at) = self.plan.binary_search(&tick) {
            self.plan.insert(at, tick);
        }
    }

    /// At the boundary before the next tick: begins the checkpoint due there, as the plan or a
    /// save of the mode's says, and sends it with the state changed since the last delta, queued
    /// behind the delta on the thread, if one is; past the seed chain's last segment, the
    /// outcome the session ends with instead, aborted. The log begins one checkpoint at a time,
    /// so one due while the last one's record is still to come waits for it. With none due,
    /// sends the state changed whenever the thread is free, so each delta holds the ticks since
    /// the last, few unless a snapshot kept the thread.
    pub(crate) fn begin(world: &mut World) -> Option<Outcome> {
        let next = world.session().log().next_tick();
        let saved = world.save_due();
        let mut checkpoints = world.resource_mut::<Checkpoints>();
        let due = saved || checkpoints.plan.first() == Some(&next);
        checkpoints.plan.retain(|&tick| tick > next);
        if !due {
            if !checkpoints.thread.pending() {
                Checkpoints::send(world, None);
            }
            return None;
        }
        let recording = world
            .resource::<Session>()
            .log()
            .begun_checkpoint()
            .is_some();
        if recording && let Err(error) = Checkpoints::settle(world) {
            world
                .resource_mut::<Faults>()
                .report(Fault::Snapshot(error));
            return None;
        }
        match world.resource_mut::<Session>().begin_checkpoint() {
            Ok(begun) => Checkpoints::send(world, Some(begun)),
            Err(CheckpointBeginError::PastSeeds) => {
                SeedsRanOut { tick: next }.log();
                return Some(Outcome::Aborted);
            }
            // A checkpoint begun at this boundary already, as a restore takes one again.
            Err(CheckpointBeginError::Log(CheckpointError::Empty)) => {}
            Err(error) => panic!("the session's own checkpoint: {}", ErrorReport::of(&error)),
        }
        None
    }

    /// Logs the record of the checkpoint the thread finished, when it did; reports its fault
    /// when its snapshot was not written.
    pub(crate) fn finish(world: &mut World) {
        let returned = world.resource_mut::<Checkpoints>().thread.take();
        if let Err(error) = Checkpoints::record(world, returned) {
            world
                .resource_mut::<Faults>()
                .report(Fault::Snapshot(error));
        }
    }

    /// Waits for every delta on the thread and queued for it, and logs the record of each
    /// checkpoint among them; an error when a snapshot was not written.
    pub(super) fn settle(world: &mut World) -> Result<(), PathError<DurableError>> {
        loop {
            let returned = world.resource_mut::<Checkpoints>().thread.wait();
            if returned.is_none() {
                return Ok(());
            }
            Checkpoints::record(world, returned)?;
        }
    }

    /// Takes `command` of a seated player: on a local server, a save makes a checkpoint due at
    /// the boundary after the next tick, unless the mode alone saves, and a load settles the
    /// checkpoints, whose latest save's segment it gives for the session to load; why not, when
    /// it refuses it.
    pub(super) fn take(
        world: &mut World,
        command: SaveCommand,
    ) -> Result<Option<u32>, SaveRefusal> {
        if !world.contains_resource::<LocalSession>() {
            return Err(SaveRefusal::NotLocal);
        }
        if !world.contains_resource::<Checkpoints>() {
            return Err(SaveRefusal::NoData);
        }
        match command {
            SaveCommand::Save => {
                let session = world.resource::<Session>();
                if session.saves().by == SaveBy::Mode {
                    return Err(SaveRefusal::ByMode);
                }
                let next = session.log().next_tick();
                world
                    .resource_mut::<Checkpoints>()
                    .request(next.after(Ticks::ONE));
                Ok(None)
            }
            SaveCommand::LoadLatest => {
                if let Err(error) = Checkpoints::settle(world) {
                    world
                        .resource_mut::<Faults>()
                        .report(Fault::Snapshot(error));
                    return Ok(None);
                }
                let latest = world.resource::<Session>().log().checkpoints().last();
                Ok(Some(latest.ok_or(SaveRefusal::NoSave)?.segment))
            }
        }
    }

    /// Takes the checkpoint begun at the boundary before the next tick again, on the main thread,
    /// as a restore does once its replay reaches it: writes its snapshot into `snapshots`, and
    /// logs its record, which `signer` signs. Nothing when no checkpoint begun starts there.
    pub(super) fn take_again(
        world: &mut World,
        snapshots: &SnapshotDir,
        signer: &ServerSigner,
    ) -> Result<(), PathError<DurableError>> {
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
        let record = world.checkpoint(&mut snapshot).expect("a checkpoint begun");
        snapshots.write(&snapshot)?;
        let signature = signer.sign_checkpoint(&record, session.log().session_id());
        Checkpoints::log(world, SignedCheckpoint { record, signature });
        Ok(())
    }

    /// Sends the thread the state changed since the last delta, with the checkpoint `begun` at
    /// the boundary before the next tick, when one is.
    fn send(world: &mut World, begun: Option<CheckpointBegun>) {
        world.resource_scope(|world, mut checkpoints: Mut<'_, Checkpoints>| {
            checkpoints.thread.send(begun, |delta| world.changes(delta));
        });
    }

    /// Logs the record `returned` brings, when it brings one; an error when its snapshot was
    /// not written.
    fn record(
        world: &mut World,
        returned: Option<Returned>,
    ) -> Result<(), PathError<DurableError>> {
        match returned {
            None | Some(Returned::Applied) => Ok(()),
            Some(Returned::Taken(signed)) => {
                Checkpoints::log(world, signed);
                Ok(())
            }
            Some(Returned::Failed(error)) => Err(error),
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
