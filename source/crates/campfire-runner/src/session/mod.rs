use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_capabilities::{MatchEnd, MatchResult, Mode, SavesData, ScriptFailures};
use campfire_common::{SegmentSeed, StateHash, Tick, Ticks};
use campfire_log::{ErrorReport, LogEvent};
use campfire_package::{ModePackages, PackageStore};
use campfire_protocol::{
    AfterLeave, Applied, Checkpoint, CheckpointBegun, CheckpointError, InputError, LogLoadError,
    Outcome, PlayerInput, RecordSink, ResultError, SeedError, ServerInput, ServerSeeds,
    SessionHeader, SessionLog, SessionResult, SessionTerms, Signature, SlotChangeKind,
    SnapshotFingerprint,
};
use campfire_sim::{
    SimRng, SimTick, SimUpdate, SlotEvent, SlotEventKind, SnapshotError, StateCopy, StateDelta,
    StateRegistry, TickInput, TickInputs, TickRate,
};

use crate::events::script_call_failed::ScriptCallFailed;
use crate::match_build::MatchBuild;
use crate::session::error::CheckpointBeginError;
use crate::session::error::ResultMismatch;
use crate::session::error::ResumeError;
use crate::session::error::ServerInputRefused;
use crate::session::error::StartError;
use crate::session_rules::SessionRules;
use crate::slot_rules::SlotRules;

pub(crate) mod error;

/// A match's session log and state types, kept as a resource in the `World` that runs the match:
/// a bare one on a verifier, Lightyear's on a server. The server records inputs as they arrive; a
/// verifier records a published log's inputs again. Either way each tick applies exactly the
/// inputs the log gives it, and draws from the seed of the segment it runs in.
#[derive(Resource, Debug)]
pub struct Session {
    /// The server seeds of the segments the session may run, secret until `reveal_seed`
    /// publishes the log.
    seeds: ServerSeeds,
    /// The segment whose seed the sim draws from.
    segment: u32,
    state: StateRegistry,
    log: SessionLog,
    /// Who the mode lets take a slot.
    rules: SlotRules,
    /// Who may ask for a save, and the mode's autosaves.
    saves: SavesData,
    /// The ticks between two autosaves, without which the mode makes none.
    autosave: Option<Ticks>,
}

/// A session's log and seeds, as `Session::into_parts` gives them.
#[derive(Debug)]
pub(crate) struct SessionParts {
    pub(crate) log: SessionLog,
    pub(crate) seeds: ServerSeeds,
}

impl Session {
    /// Prepares `world` for the match of `log`'s header, of the mode `packages` holds, at the
    /// header's tick rate and with the randomness of the first segment's seed of `seeds` and the
    /// players' contributions; starts the match; and inserts the session, which records into
    /// `log` from its first tick. An error when the terms name another release, mode or
    /// dependencies than this release and `packages`, a tick rate outside the mode's range, or
    /// more slots than the mode's teams have, when `seeds` are not the seeds of the chain the
    /// header commits to or do not reach the last segment of `log`, when a change of a slot's
    /// controller the log holds, as a published log replayed does, breaks the mode's
    /// `[players]`, or when the packages do not load into the match.
    pub fn start(
        world: &mut World,
        log: SessionLog,
        seeds: ServerSeeds,
        packages: &ModePackages,
    ) -> Result<(), StartError> {
        assert_eq!(
            log.next_tick(),
            Tick::new(0),
            "a session starts before its first tick"
        );
        let session = Session::build(world, log, seeds, packages, 0)?;
        Mode::start(world).map_err(StartError::MatchStart)?;
        world.insert_resource(session);
        Ok(())
    }

    /// Prepares `world` for the match of `log`, rewound to replay its ticks, from the checkpoint
    /// of it that starts `segment`, as a restore does: builds the match as `start` does, but
    /// starts no mode, as the state is the checkpoint's `snapshot`; and seals the log's ticks
    /// before the checkpoint without running them, so the next tick the session runs is the
    /// checkpoint's. An error as `start` gives one, and when the log holds no such checkpoint,
    /// when `snapshot` is not the one the checkpoint fingerprints, when it does not restore,
    /// or restores to another state hash than the checkpoint's.
    pub fn resume(
        world: &mut World,
        log: SessionLog,
        seeds: ServerSeeds,
        packages: &ModePackages,
        segment: u32,
        snapshot: &[u8],
    ) -> Result<(), ResumeError> {
        assert_eq!(
            log.next_tick(),
            Tick::new(0),
            "a resumed log replays from its first tick"
        );
        let record = log
            .checkpoints()
            .find(|record| record.segment == segment)
            .ok_or(ResumeError::NoCheckpoint)?;
        let (tick, state_hash) = (record.tick, record.state_hash);
        if SnapshotFingerprint::of(snapshot) != record.snapshot {
            return Err(ResumeError::Fingerprint);
        }
        let mut session =
            Session::build(world, log, seeds, packages, segment).map_err(ResumeError::Start)?;
        session
            .state
            .restore(snapshot, world)
            .map_err(ResumeError::Snapshot)?;
        if session.state.hash(world) != state_hash {
            return Err(ResumeError::StateHash);
        }
        session.log.seal_until(tick);
        world.insert_resource(session);
        Ok(())
    }

    /// Builds the match of `log`'s header in `world`, of the mode `packages` holds, at the
    /// header's tick rate and with the randomness of `segment`'s seed; the session of `log`,
    /// drawing from `segment`. An error as `start` gives one, but for the mode's start.
    fn build(
        world: &mut World,
        log: SessionLog,
        seeds: ServerSeeds,
        packages: &ModePackages,
        segment: u32,
    ) -> Result<Session, StartError> {
        let header = log.header();
        SessionRules::of(packages)
            .check(&header.terms)
            .map_err(StartError::Terms)?;
        if seeds.last() < log.segment() {
            return Err(StartError::Seed(SeedError::NotRevealed));
        }
        let server_seed = seeds
            .seed(segment)
            .expect("the seeds reach the log's last segment");
        let seed = header
            .segment_seed(segment, &server_seed)
            .map_err(StartError::Seed)?;
        SimUpdate::prepare(world, seed, TickRate::new(header.terms.tick_hz));
        let mut schedule = SimUpdate::schedule();
        let mut state = StateRegistry::new();
        let rules = SlotRules::new(packages.data().players);
        for &change in log.changes() {
            rules
                .check(change.kind)
                .map_err(|error| StartError::SlotRule { change, error })?;
        }
        let players = u32::try_from(header.slots.len()).expect("the log counts slots in u32");
        MatchBuild::run(packages, world, &mut schedule, &mut state, players);
        world.add_schedule(schedule);
        let saves = packages.data().saves;
        let rate = TickRate::new(header.terms.tick_hz);
        let autosave = saves
            .autosave_ms
            .map(|ms| rate.ticks(u64::from(ms.get())).expect("a tick at least"));
        Ok(Session {
            seeds,
            segment,
            state,
            log,
            rules,
            saves,
            autosave,
        })
    }

    /// The state hash of `snapshot` restored in a match of the mode `packages` holds, as the
    /// session of `header` plays it, as a verifier checks a checkpoint's snapshot; an error for
    /// bytes that do not restore.
    pub fn snapshot_hash(
        packages: &ModePackages,
        header: &SessionHeader,
        snapshot: &[u8],
    ) -> Result<StateHash, SnapshotError> {
        let mut world = World::new();
        // The seed is not state, so no seed changes what the restore gives.
        let seed = SegmentSeed::new([0; 32]);
        SimUpdate::prepare(&mut world, seed, TickRate::new(header.terms.tick_hz));
        let mut schedule = SimUpdate::schedule();
        let mut state = StateRegistry::new();
        let slots = u32::try_from(header.slots.len()).expect("the log counts slots in u32");
        MatchBuild::run(packages, &mut world, &mut schedule, &mut state, slots);
        state.restore(snapshot, &mut world)?;
        Ok(state.hash(&world))
    }

    /// The mode and the dependencies `terms` name, from `store`, as a verifier holds them; an
    /// error for terms of another release first, as its packages may not read in this one.
    pub fn packages(
        store: &PackageStore,
        terms: &SessionTerms,
    ) -> Result<ModePackages, StartError> {
        SessionRules::check_release(terms).map_err(StartError::Terms)?;
        ModePackages::from_store(store, terms.mode, &terms.dependencies)
            .map_err(StartError::Packages)
    }

    /// Logs a player's packet before the next tick; see `SessionLog::record`.
    pub fn record<'a, I>(
        &mut self,
        inputs: I,
        signature: &Signature,
        applied: &mut Vec<Applied>,
    ) -> Result<(), InputError>
    where
        I: IntoIterator<Item = PlayerInput<'a>>,
        I::IntoIter: Clone,
    {
        self.log.record(inputs, signature, applied)
    }

    /// Logs a server input before the next tick, its `signature` the server key's at its place;
    /// see `SessionLog::record_server`. The mode's `[players]` refuses a change of a slot's
    /// controller it does not allow, before the log takes it.
    pub fn record_server(
        &mut self,
        input: ServerInput<'_>,
        signature: &Signature,
    ) -> Result<(), ServerInputRefused> {
        if let Some(change) = self
            .log
            .change_of(&input)
            .map_err(ServerInputRefused::Log)?
        {
            self.rules.check(change).map_err(ServerInputRefused::Rule)?;
        }
        self.log
            .record_server(input, signature)
            .map_err(ServerInputRefused::Log)
    }

    /// Seals the next tick in the log of the session in `world` and runs it with the inputs and
    /// the changes of a slot's controller applied in it, drawing from the seed of the segment
    /// that starts there when one does, and logs each script call of the tick that failed.
    pub fn run_tick(world: &mut World) {
        world.resource_scope(|world, mut session: Mut<'_, Session>| {
            debug_assert_eq!(
                world.resource::<SimTick>().start(),
                session.log.next_tick(),
                "the sim and the log are at the same tick"
            );
            let next = session.log.next_tick();
            if let Some(segment) = session
                .log
                .segment_starting(next)
                .filter(|&segment| segment > session.segment)
            {
                let server_seed = session
                    .seeds
                    .seed(segment)
                    .expect("the seeds reach the log's last segment");
                let seed = session
                    .log
                    .header()
                    .segment_seed(segment, &server_seed)
                    .expect("seeds whose first checks give every segment's");
                world.insert_resource(SimRng::new(seed));
                session.segment = segment;
            }
            let mut inputs = world.resource_mut::<TickInputs>();
            for input in session.log.seal_tick() {
                inputs.push(TickInput {
                    slot: input.slot,
                    payload: input.payload,
                });
            }
            for change in session.log.sealed_changes() {
                let kind = match change.kind {
                    SlotChangeKind::Joined { .. } => SlotEventKind::Joined,
                    SlotChangeKind::Left { .. } => SlotEventKind::Left,
                };
                inputs.push_slot_event(SlotEvent {
                    slot: change.slot,
                    kind,
                });
            }
        });
        let tick = world.resource::<SimTick>().start();
        world.run_schedule(SimUpdate);
        if let Some(failures) = world.get_non_send::<ScriptFailures>() {
            for failure in failures.get() {
                ScriptCallFailed {
                    tick,
                    unit: failure.unit,
                    hook: failure.hook,
                    error: ErrorReport::of(&failure.error).to_string(),
                }
                .log();
            }
        }
    }

    /// Goes back to the save that starts segment `segment`; see `SessionLog::load`. The match's
    /// world is then stale: a new one resumes from the save.
    pub fn load(&mut self, segment: u32) -> Result<(), LogLoadError> {
        self.log.load(segment)
    }

    /// Who may ask for a save, and the mode's autosaves, as its `[saves]` says.
    pub const fn saves(&self) -> SavesData {
        self.saves
    }

    /// Whether a save is due at the boundary before the next tick of the session in `world`:
    /// the mode asked for one in the tick before, by `ctx.save()`, or the boundary is at a
    /// multiple of the mode's `[saves] autosave_ms` from the session's start.
    pub fn save_due(&self, world: &World) -> bool {
        let next = self.log.next_tick();
        let asked = Mode::save_asked(world) == Some(next);
        let autosave = self
            .autosave
            .is_some_and(|every| next.get() > 0 && next.get().is_multiple_of(every.get()));
        asked || autosave
    }

    /// What the slot of a player who leaves becomes, as the mode's `[players] leaver` says.
    pub const fn after_leave(&self) -> AfterLeave {
        self.rules.after_leave()
    }

    /// See `SessionLog::advance_durable`.
    pub fn advance_durable(&mut self, durable: u64) {
        self.log.advance_durable(durable);
    }

    /// Keeps `sink` as the journal of the session's log, a new one: see
    /// `SessionLog::keep_journal`.
    pub fn keep_journal(&mut self, sink: Box<dyn RecordSink>) {
        self.log.keep_journal(sink);
    }

    /// Keeps `sink` as the journal of the session's log, which holds every record of it; see
    /// `SessionLog::resume_journal`.
    pub fn resume_journal(&mut self, sink: Box<dyn RecordSink>) {
        self.log.resume_journal(sink);
    }

    /// Begins a checkpoint at the boundary before the next tick, which starts the segment after
    /// the log's last: the next tick draws from its seed. An error past the seed chain's last
    /// segment, and as `SessionLog::begin_checkpoint` gives one.
    pub fn begin_checkpoint(&mut self) -> Result<CheckpointBegun, CheckpointBeginError> {
        let segment = self.log.segment().checked_add(1);
        if segment
            .and_then(|segment| self.seeds.seed(segment))
            .is_none()
        {
            return Err(CheckpointBeginError::PastSeeds);
        }
        self.log
            .begin_checkpoint()
            .map_err(CheckpointBeginError::Log)
    }

    /// The record of the checkpoint the log began and has no record of, of the state of `world`
    /// at its boundary, its snapshot written into `snapshot`, which is cleared first; none when
    /// the log began none.
    pub fn checkpoint(&self, world: &World, snapshot: &mut Vec<u8>) -> Option<Checkpoint> {
        let begun = self.log.begun_checkpoint()?;
        debug_assert_eq!(
            world.resource::<SimTick>().start(),
            begun.tick,
            "the state stands at the checkpoint's boundary"
        );
        self.state.snapshot(world, snapshot);
        Some(Checkpoint {
            segment: begun.segment,
            tick: begun.tick,
            state_hash: self.state.hash(world),
            snapshot: SnapshotFingerprint::of(snapshot),
            carry: begun.carry.clone(),
        })
    }

    /// Logs the record of the checkpoint begun, with the server key's `signature` over it; see
    /// `SessionLog::record_checkpoint`.
    pub fn record_checkpoint(
        &mut self,
        record: Checkpoint,
        signature: &Signature,
    ) -> Result<(), CheckpointError> {
        self.log.record_checkpoint(record, signature)
    }

    /// How the match in `world` ended: as the mode ended it, or aborted when it did not.
    pub fn outcome(world: &World) -> Outcome {
        match world.get_resource::<MatchEnd>().map(|end| end.result()) {
            Some(MatchResult::Won(team)) => Outcome::Won { team: team.index() },
            Some(MatchResult::Draw) => Outcome::Draw,
            None => Outcome::Aborted,
        }
    }

    /// The result that ends the session before the next tick as `outcome`, with the state hash
    /// of `world`.
    pub fn result(&self, world: &World, outcome: Outcome) -> SessionResult {
        SessionResult {
            tick: self.log.next_tick(),
            outcome,
            state_hash: self.state.hash(world),
        }
    }

    /// Logs the result with the server key's `signature` over it; see
    /// `SessionLog::record_result`.
    pub fn record_result(
        &mut self,
        result: SessionResult,
        signature: &Signature,
    ) -> Result<(), ResultError> {
        self.log.record_result(result, signature)
    }

    /// Whether the log's result, once every tick before it ran, holds for `world`: its state
    /// hash is the state's, and its outcome the one the mode ended the match with, unless the
    /// session aborted.
    pub fn check_result(&self, world: &World) -> Result<(), ResultMismatch> {
        let Some(result) = self.log.result() else {
            return Ok(());
        };
        assert_eq!(
            result.tick,
            self.log.next_tick(),
            "a result is checked once every tick before it ran"
        );
        let replayed = self.state.hash(world);
        if result.state_hash != replayed {
            return Err(ResultMismatch::Hash {
                logged: result.state_hash,
                replayed,
            });
        }
        let ended = Session::outcome(world);
        if result.outcome != Outcome::Aborted && result.outcome != ended {
            return Err(ResultMismatch::Outcome {
                logged: result.outcome,
                ended,
            });
        }
        Ok(())
    }

    /// Publishes the log by adding the server seed of its last segment, which reveals every
    /// earlier one.
    pub fn reveal_seed(&mut self) {
        let last = self
            .seeds
            .seed(self.log.segment())
            .expect("the seeds reach the log's last segment");
        self.log.reveal_seed(last);
    }

    pub fn state_hash(&self, world: &World) -> StateHash {
        self.state.hash(world)
    }

    /// The session's log and seeds, its match's world left.
    pub(crate) fn into_parts(self) -> SessionParts {
        SessionParts {
            log: self.log,
            seeds: self.seeds,
        }
    }

    /// The registry of the match's state types.
    pub const fn registry(&self) -> &StateRegistry {
        &self.state
    }

    /// Starts recording the changes of the state of the session in `world`, and writes its whole
    /// state into `delta`; see `StateRegistry::track`.
    pub fn track(world: &mut World, delta: &mut StateDelta) {
        world.resource_scope(|world, session: Mut<'_, Session>| {
            session.state.track(world, delta);
        });
    }

    /// Writes into `delta` the state of the session in `world` that changed since the last
    /// copy; see `StateRegistry::changes`.
    pub fn changes(world: &mut World, delta: &mut StateDelta) {
        world.resource_scope(|world, session: Mut<'_, Session>| {
            session.state.changes(world, delta);
        });
    }

    /// A copy of the state of the session in `world`, which follows it by what changed: see
    /// `StateCopy`.
    pub fn copy_state(world: &mut World) -> StateCopy {
        world.resource_scope(|world, session: Mut<'_, Session>| {
            StateCopy::new(&session.state, world)
        })
    }

    /// Brings `copy` up to the state of the session in `world`.
    pub fn follow(world: &mut World, copy: &mut StateCopy) {
        world.resource_scope(|world, session: Mut<'_, Session>| {
            copy.follow(&session.state, world);
        });
    }

    pub const fn log(&self) -> &SessionLog {
        &self.log
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_common::StateHash;
    use campfire_sim::TypeHash;

    use crate::session::Session;

    impl Session {
        /// The hash of the state of `world`, and in `per_type` each state type's own.
        pub(crate) fn state_hash_by_type(
            &self,
            world: &World,
            per_type: &mut Vec<TypeHash>,
        ) -> StateHash {
            self.state.hash_by_type(world, per_type)
        }
    }
}
