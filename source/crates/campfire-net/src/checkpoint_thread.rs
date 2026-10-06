use bevy_ecs::world::World;
use campfire_protocol::{Checkpoint, CheckpointBegun, SessionId, Signature};
use campfire_sim::{EntityIndex, StateDelta, StateRegistry};
use campfire_store::{DurableError, Exchange};

use crate::server_signer::ServerSigner;
use crate::session_dir::snapshots::Snapshots;

/// The thread that takes a session's checkpoints off the main thread. It holds its own copy of
/// the match's state, which starts as a full copy and which each delta the main thread sends
/// brings up to the boundary it was made at; for a checkpoint, it then snapshots and hashes that
/// copy, writes the snapshot durably into the session's `snapshots`, named by its fingerprint in
/// hex, and signs the record, which it gives back for the main thread to log. It takes one delta
/// at a time, and gives each back for the next.
#[derive(Debug)]
pub(crate) struct CheckpointThread {
    exchange: Exchange<Job, Returned>,
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
    Failed(DurableError),
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
    snapshots: Snapshots,
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
        snapshots: Snapshots,
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
        CheckpointThread { exchange }
    }

    /// Whether a delta was sent and has not come back.
    pub(crate) const fn pending(&self) -> bool {
        self.exchange.pending()
    }

    /// Sends the state changed since the last delta, which `write` writes into a delta, the one
    /// the last gave back; with the checkpoint `begun` at the boundary it is made at, when one
    /// was.
    pub(crate) fn send(
        &mut self,
        begun: Option<CheckpointBegun>,
        write: impl FnOnce(&mut StateDelta),
    ) {
        self.exchange.send(|job| {
            job.begun = begun;
            write(&mut job.delta);
        });
    }

    /// What the delta sent came back as, once it did.
    pub(crate) fn take(&mut self) -> Option<Returned> {
        self.exchange.take()
    }

    /// What the delta sent came back as, waiting for it; none when none was sent.
    pub(crate) fn wait(&mut self) -> Option<Returned> {
        self.exchange.wait()
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
    fn take(&mut self, begun: CheckpointBegun) -> Result<SignedCheckpoint, DurableError> {
        self.registry.snapshot(&self.world, &mut self.snapshot);
        let fingerprint = self.snapshots.write(&self.snapshot)?;
        let record = Checkpoint {
            segment: begun.segment,
            tick: begun.tick,
            state_hash: self.registry.hash(&self.world),
            snapshot: fingerprint,
            carry: begun.carry,
        };
        let signature = self.signer.sign_checkpoint(&record, self.session_id);
        Ok(SignedCheckpoint { record, signature })
    }
}
