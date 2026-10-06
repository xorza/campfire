use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use bevy_ecs::world::World;
use campfire_protocol::{
    Checkpoint, CheckpointBegun, DurableError, DurableFile, SessionId, Signature,
    SnapshotFingerprint,
};
use campfire_sim::{EntityIndex, StateDelta, StateRegistry};

use crate::server_signer::ServerSigner;

/// The thread that takes a session's checkpoints off the main thread. It holds its own copy of
/// the match's state, which starts as a full copy and which each delta the main thread sends
/// brings up to the boundary it was made at; for a checkpoint, it then snapshots and hashes that
/// copy, writes the snapshot durably into the session's `snapshots`, named by its fingerprint in
/// hex, and signs the record, which it gives back for the main thread to log. It takes one delta
/// at a time.
#[derive(Debug)]
pub(crate) struct CheckpointThread {
    jobs: Option<Sender<Job>>,
    done: Mutex<Receiver<Done>>,
    thread: Option<JoinHandle<()>>,
    /// Whether a delta was sent and has not come back.
    pending: bool,
    /// The delta the last checkpoint gave back, for the next.
    spare: StateDelta,
}

/// What the main thread sends the checkpoint thread.
#[derive(Debug)]
enum Job {
    /// The whole state, which the thread's copy starts as.
    Base(StateDelta),
    /// The state changed since the last job, and the checkpoint begun at the boundary the delta
    /// was made at, when one was.
    Changes {
        begun: Option<CheckpointBegun>,
        delta: StateDelta,
    },
}

/// What the checkpoint thread gives back for a delta, with the delta, to send again.
#[derive(Debug)]
struct Done {
    returned: Returned,
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

/// What the checkpoint thread works with.
#[derive(Debug)]
struct Worker {
    registry: StateRegistry,
    session_id: SessionId,
    snapshots: PathBuf,
    signer: ServerSigner,
    world: World,
    /// The snapshot of the last checkpoint, kept between checkpoints.
    snapshot: Vec<u8>,
}

impl CheckpointThread {
    /// Starts the thread of the session of `session_id`, whose state types `registry` holds and
    /// whose whole state `base` holds, writing snapshots into `snapshots` and signing with
    /// `signer`.
    pub(crate) fn start(
        registry: StateRegistry,
        base: StateDelta,
        session_id: SessionId,
        snapshots: PathBuf,
        signer: ServerSigner,
    ) -> CheckpointThread {
        let (jobs, received) = mpsc::channel();
        let (sent, done) = mpsc::channel();
        let mut world = World::new();
        world.init_resource::<EntityIndex>();
        let worker = Worker {
            registry,
            session_id,
            snapshots,
            signer,
            world,
            snapshot: Vec::new(),
        };
        let thread = thread::Builder::new()
            .name("checkpoints".to_owned())
            .spawn(move || worker.run(&received, &sent))
            .expect("the OS starts a thread");
        jobs.send(Job::Base(base))
            .expect("the checkpoint thread runs");
        CheckpointThread {
            jobs: Some(jobs),
            done: Mutex::new(done),
            thread: Some(thread),
            pending: false,
            spare: StateDelta::default(),
        }
    }

    /// Writes `snapshot` durably into `dir`, made when missing, named by its fingerprint in hex;
    /// the fingerprint.
    pub(crate) fn write_snapshot(
        dir: &Path,
        snapshot: &[u8],
    ) -> Result<SnapshotFingerprint, DurableError> {
        let fingerprint = SnapshotFingerprint::of(snapshot);
        DurableFile::create_dir(dir)?;
        DurableFile::write(&dir.join(fingerprint.to_string()), snapshot)?;
        Ok(fingerprint)
    }

    /// Whether a delta was sent and has not come back.
    pub(crate) const fn pending(&self) -> bool {
        self.pending
    }

    /// Sends the state changed since the last delta, which `write` writes into a delta, the one
    /// the last gave back; with the checkpoint `begun` at the boundary it is made at, when one
    /// was.
    pub(crate) fn send(
        &mut self,
        begun: Option<CheckpointBegun>,
        write: impl FnOnce(&mut StateDelta),
    ) {
        assert!(!self.pending, "one delta at a time");
        let mut delta = mem::take(&mut self.spare);
        write(&mut delta);
        self.jobs
            .as_ref()
            .expect("a running thread")
            .send(Job::Changes { begun, delta })
            .expect("the checkpoint thread runs");
        self.pending = true;
    }

    /// What the delta sent came back as, once it did; waiting for it when `wait` says so.
    pub(crate) fn done(&mut self, wait: bool) -> Option<Returned> {
        if !self.pending {
            return None;
        }
        let receiver = self
            .done
            .get_mut()
            .expect("the main thread does not panic holding the receiver");
        let done = if wait {
            receiver
                .recv()
                .expect("the checkpoint thread runs while a checkpoint is pending")
        } else {
            receiver.try_recv().ok()?
        };
        self.pending = false;
        self.spare = done.delta;
        Some(done.returned)
    }
}

impl Worker {
    /// Takes each job until the main thread closes the channel.
    fn run(mut self, jobs: &Receiver<Job>, done: &Sender<Done>) {
        while let Ok(job) = jobs.recv() {
            match job {
                Job::Base(delta) => self.registry.apply(&delta, &mut self.world),
                Job::Changes { begun, delta } => {
                    self.registry.apply(&delta, &mut self.world);
                    let returned = match begun {
                        None => Returned::Applied,
                        Some(begun) => self
                            .take(begun)
                            .map_or_else(Returned::Failed, Returned::Taken),
                    };
                    if done.send(Done { returned, delta }).is_err() {
                        return;
                    }
                }
            }
        }
    }

    /// The record of `begun`, of the copy as it stands, signed once its snapshot is written.
    fn take(&mut self, begun: CheckpointBegun) -> Result<SignedCheckpoint, DurableError> {
        self.registry.snapshot(&self.world, &mut self.snapshot);
        let fingerprint = CheckpointThread::write_snapshot(&self.snapshots, &self.snapshot)?;
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

impl Drop for CheckpointThread {
    fn drop(&mut self) {
        drop(self.jobs.take());
        if let Some(thread) = self.thread.take() {
            thread.join().expect("the checkpoint thread does not panic");
        }
    }
}
