use std::collections::VecDeque;

use bevy_ecs::world::World;
use campfire_protocol::{Checkpoint, CheckpointBegun, SessionId, Signature};
use campfire_sim::{EntityIndex, StateDelta, StateRegistry};
use campfire_store::{DurableError, Exchange, PathError};

use crate::sim_server::server_signer::ServerSigner;
use crate::sim_server::session_dir::snapshot_dir::SnapshotDir;

/// The thread that takes a session's checkpoints off the main thread. It holds its own copy of
/// the match's state, which starts as a full copy and which each delta the main thread sends
/// brings up to the boundary it was made at; for a checkpoint, it then snapshots and hashes that
/// copy, writes the snapshot durably into the session's `snapshots`, named by its fingerprint in
/// hex, and signs the record, which it gives back for the main thread to log. It takes one delta
/// at a time, and gives each back for the next; one made while another is on the thread waits in
/// order, so the main thread never waits for the thread to send.
#[derive(Debug)]
pub(crate) struct CheckpointThread {
    exchange: Exchange<Job, Returned>,
    /// The deltas made while one was on the thread, in order: each goes as the one before comes
    /// back, so a queued one means one is on the thread.
    queued: VecDeque<Job>,
    /// Jobs given back, whose buffers the next queued delta reuses.
    spares: Vec<Job>,
}

/// What the main thread sends the checkpoint thread: the state changed since the last job, the
/// whole state for the first, and the checkpoint begun at the boundary the delta was made at,
/// when one was.
#[derive(Debug, Default)]
struct Job {
    begun: Option<CheckpointBegun>,
    delta: StateDelta,
}

/// What a delta sent came back as.
#[derive(Debug)]
pub(crate) enum Returned {
    /// The copy follows it; no checkpoint was begun there.
    Applied,
    /// The checkpoint begun there: its record, signed, its snapshot written.
    Taken(SignedCheckpoint),
    /// The checkpoint begun there, whose snapshot was not written.
    Failed(PathError<DurableError>),
}

/// A checkpoint's record and the server key's signature over it, its snapshot written.
#[derive(Debug)]
pub(crate) struct SignedCheckpoint {
    pub(crate) record: Checkpoint,
    pub(crate) signature: Signature,
}

/// The checkpoint thread's copy of the match's state, and what it takes checkpoints with.
#[derive(Debug)]
struct CheckpointCopy {
    registry: StateRegistry,
    session_id: SessionId,
    snapshots: SnapshotDir,
    signer: ServerSigner,
    world: World,
    /// The snapshot of the last checkpoint, kept between checkpoints.
    snapshot: Vec<u8>,
}

impl CheckpointThread {
    /// Starts the thread of the session of `session_id`, whose state types `registry` holds and
    /// whose whole state `base` holds, writing snapshots into `snapshots` and signing with
    /// `signer`. The base is the first delta sent.
    pub(crate) fn start(
        registry: StateRegistry,
        base: StateDelta,
        session_id: SessionId,
        snapshots: SnapshotDir,
        signer: ServerSigner,
    ) -> CheckpointThread {
        let mut world = World::new();
        world.init_resource::<EntityIndex>();
        let mut copy = CheckpointCopy {
            registry,
            session_id,
            snapshots,
            signer,
            world,
            snapshot: Vec::new(),
        };
        let mut exchange = Exchange::start("checkpoints", move |job: &mut Job| copy.run(job));
        exchange.send(|job| job.delta = base);
        CheckpointThread {
            exchange,
            queued: VecDeque::new(),
            spares: Vec::new(),
        }
    }

    /// Whether a delta was sent and has not come back.
    pub(crate) const fn pending(&self) -> bool {
        self.exchange.pending()
    }

    /// Sends the state changed since the last delta, which `write` writes into a delta, the
    /// buffers of one given back; with the checkpoint `begun` at the boundary it is made at, when
    /// one was. With a delta on the thread, it waits behind it.
    pub(crate) fn send(
        &mut self,
        begun: Option<CheckpointBegun>,
        write: impl FnOnce(&mut StateDelta),
    ) {
        if !self.exchange.pending() {
            debug_assert!(
                self.queued.is_empty(),
                "a queued delta waits for one on the thread"
            );
            self.exchange.send(|job| {
                job.begun = begun;
                write(&mut job.delta);
            });
            return;
        }
        let mut job = self.spares.pop().unwrap_or_default();
        job.begun = begun;
        write(&mut job.delta);
        self.queued.push_back(job);
    }

    /// What the delta on the thread came back as, once it did; the next queued one then goes.
    pub(crate) fn take(&mut self) -> Option<Returned> {
        let returned = self.exchange.take()?;
        self.send_queued();
        Some(returned)
    }

    /// What the delta on the thread came back as, waiting for it; none when none is there. The
    /// next queued one then goes.
    pub(crate) fn wait(&mut self) -> Option<Returned> {
        let returned = self.exchange.wait()?;
        self.send_queued();
        Some(returned)
    }

    fn send_queued(&mut self) {
        if let Some(next) = self.queued.pop_front() {
            let spare = self.exchange.send_filled(next);
            self.spares.push(spare);
        }
    }
}

impl CheckpointCopy {
    /// Brings the copy up to `job`'s delta, and takes the checkpoint begun there, when one was.
    fn run(&mut self, job: &mut Job) -> Returned {
        self.registry.apply(&job.delta, &mut self.world);
        match job.begun.take() {
            None => Returned::Applied,
            Some(begun) => self
                .take(begun)
                .map_or_else(Returned::Failed, Returned::Taken),
        }
    }

    /// The record of `begun`, of the copy as it stands, signed once its snapshot is written.
    fn take(
        &mut self,
        begun: CheckpointBegun,
    ) -> Result<SignedCheckpoint, PathError<DurableError>> {
        let state_hash = self.registry.snapshot(&self.world, &mut self.snapshot);
        let fingerprint = self.snapshots.write(&self.snapshot)?;
        let record = Checkpoint {
            segment: begun.segment,
            tick: begun.tick,
            state_hash,
            snapshot: fingerprint,
            carry: begun.carry,
        };
        let signature = self.signer.sign_checkpoint(&record, self.session_id);
        Ok(SignedCheckpoint { record, signature })
    }
}

#[cfg(test)]
mod tests {
    use campfire_common::Tick;
    use campfire_math::{Num, Vec3};
    use campfire_protocol::LogCarry;
    use campfire_protocol::internals::TestKey;
    use campfire_sim::{IdAllocator, Position};
    use campfire_store::Scratch;

    use super::*;

    #[test]
    fn deltas_sent_while_one_is_on_the_thread_follow_it_in_order() {
        // The base goes first, and stays on the thread until its answer is taken; the next two
        // deltas, each a unit more, and the checkpoint after the second, queue behind it. They
        // come back in order, and the checkpoint holds the state the main thread had then.
        let registry = StateRegistry::new();
        let mut world = World::new();
        world.init_resource::<EntityIndex>();
        world.init_resource::<IdAllocator>();
        let mut base = StateDelta::default();
        registry.track(&mut world, &mut base);
        let scratch = Scratch::new();
        let signer = ServerSigner::new(TestKey::server(), |bytes| bytes.fill(1));
        let session_id = SessionId::new([2; 32]);
        let snapshots = SnapshotDir(scratch.path("snapshots"));
        let mut thread =
            CheckpointThread::start(registry.clone(), base, session_id, snapshots, signer);
        for x in 1..=2 {
            let id = world.resource_mut::<IdAllocator>().allocate();
            world.spawn((
                id,
                Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::ZERO)).unwrap(),
            ));
            thread.send(None, |delta| registry.changes(&mut world, delta));
        }
        let begun = CheckpointBegun {
            segment: 1,
            tick: Tick::new(3),
            carry: LogCarry::empty(),
        };
        thread.send(Some(begun), |delta| registry.changes(&mut world, delta));
        assert_eq!(thread.queued.len(), 3);
        let mut answers = Vec::new();
        while let Some(returned) = thread.wait() {
            answers.push(returned);
        }
        let [
            Returned::Applied,
            Returned::Applied,
            Returned::Applied,
            Returned::Taken(taken),
        ] = answers.as_slice()
        else {
            panic!("three deltas applied, then the checkpoint: {answers:?}");
        };
        assert_eq!(taken.record.state_hash, registry.hash(&world));
        assert_eq!(taken.record.tick, Tick::new(3));
        assert!(thread.queued.is_empty() && !thread.pending());
    }
}
