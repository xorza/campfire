use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_capabilities::{MatchEnd, MatchResult, Mode, ScriptFailures};
use campfire_common::{SegmentSeed, StateHash, Tick};
use campfire_log::LogEvent;
use campfire_package::{ModePackages, PackageStore};
use campfire_protocol::{
    AfterLeave, Applied, Checkpoint, CheckpointError, InputError, Journal, Outcome, PlayerInput,
    ResultError, SeedError, ServerInput, ServerSeeds, SessionHeader, SessionLog, SessionResult,
    SessionTerms, Signature, SlotChangeKind, SnapshotFingerprint,
};
use campfire_sim::{
    SimRng, SimTick, SimUpdate, SlotEvent, SlotEventKind, SnapshotError, StateRegistry, TickInput,
    TickInputs, TickRate,
};

use crate::error::{ResultMismatch, ServerInputRefused, StartError};
use crate::match_build::MatchBuild;
use crate::script_call_failed::ScriptCallFailed;
use crate::session_rules::SessionRules;
use crate::slot_rules::SlotRules;

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
        let header = log.header();
        SessionRules::of(packages)
            .check(&header.terms)
            .map_err(StartError::Terms)?;
        if seeds.last() < log.segment() {
            return Err(StartError::Seed(SeedError::NotRevealed));
        }
        let first = seeds.seed(0).expect("every chain has a first segment");
        let seed = header.segment_seed(0, &first).map_err(StartError::Seed)?;
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
        Mode::start(world).map_err(StartError::MatchStart)?;
        world.insert_resource(Session {
            seeds,
            segment: 0,
            state,
            log,
            rules,
        });
        Ok(())
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
        input: ServerInput,
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
    /// the changes of a slot's controller applied in it, drawing from the seed of the segment that starts there when one does, and
    /// logs each script call of the tick that failed.
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
                .checkpoint_at(next)
                .map(|record| record.segment)
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
                    error: failure.error.to_string(),
                }
                .log();
            }
        }
    }

    /// What the slot of a player who leaves becomes, as the mode's `[players] leaver` says.
    pub const fn after_leave(&self) -> AfterLeave {
        self.rules.after_leave()
    }

    /// See `SessionLog::advance_durable`.
    pub fn advance_durable(&mut self) {
        self.log.advance_durable();
    }

    /// Keeps `journal`, a new one, for the session's log: see `SessionLog::keep_journal`.
    pub fn keep_journal(&mut self, journal: Journal) {
        self.log.keep_journal(journal);
    }

    /// Keeps `journal`, which holds every record of the session's log; see
    /// `SessionLog::resume_journal`.
    pub fn resume_journal(&mut self, journal: Journal) {
        self.log.resume_journal(journal);
    }

    /// The checkpoint record of the boundary before the next tick, which starts the segment after
    /// the log's last, its snapshot of `world` written into `snapshot`, which is cleared first;
    /// none when the seed chain has no segment after the last.
    pub fn checkpoint(&self, world: &World, snapshot: &mut Vec<u8>) -> Option<Checkpoint> {
        let segment = self.log.segment().checked_add(1)?;
        self.seeds.seed(segment)?;
        self.state.snapshot(world, snapshot);
        Some(Checkpoint {
            segment,
            tick: self.log.next_tick(),
            state_hash: self.state.hash(world),
            snapshot: SnapshotFingerprint::of(snapshot),
            carry: self.log.carry(),
        })
    }

    /// Logs a checkpoint record with the server key's `signature` over it; see
    /// `SessionLog::record_checkpoint`. The next tick draws from the new segment's seed.
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
