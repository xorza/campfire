use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_capabilities::{Match, ReleaseWorld, SavesData};
use campfire_common::{SegmentSeed, StateHash, Ticks};
use campfire_package::{ModePackages, PackageStore};
use campfire_protocol::secp256k1::{Keypair, Secp256k1, Signing};
use campfire_protocol::{
    AfterLeave, Applied, Checkpoint, CheckpointBegun, CheckpointError, InputError, LogLoadError,
    PlayerInput, RecordSink, ResultError, SeedError, ServerInput, ServerSeeds, SessionHeader,
    SessionLog, SessionResult, SessionTerms, Signature,
};
use campfire_sim::{SimWorld, SnapshotError, TickRate};

use crate::match_build::MatchBuild;
use crate::session::error::{CheckpointBeginError, ServerInputRefused, StartError};
use crate::session_rules::SessionRules;
use crate::slot_rules::SlotRules;

pub(crate) mod error;
pub(crate) mod session_match;
pub(crate) mod session_world;

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
        let rules = SlotRules::new(packages.data().players);
        for &change in log.changes() {
            rules
                .check(change.kind)
                .map_err(|error| StartError::SlotRule { change, error })?;
        }
        let players = u32::try_from(header.slots.len()).expect("the log counts slots in u32");
        let rate = TickRate::new(header.terms.tick_hz);
        world.build_match(MatchBuild::spec(packages, seed, rate, players));
        MatchBuild::install_mode(packages, world, players);
        let saves = packages.data().saves;
        let autosave = saves
            .autosave_ms
            .map(|ms| rate.ticks(u64::from(ms.get())).expect("a tick at least"));
        Ok(Session {
            seeds,
            segment,
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
        // The seed is not state, so no seed changes what the restore gives.
        let seed = SegmentSeed::new([0; 32]);
        let slots = u32::try_from(header.slots.len()).expect("the log counts slots in u32");
        let rate = TickRate::new(header.terms.tick_hz);
        let mut game = Match::new(MatchBuild::spec(packages, seed, rate, slots));
        MatchBuild::install_mode(packages, &mut game, slots);
        game.restore(snapshot)?;
        Ok(game.state_hash())
    }

    /// The mode and the dependencies `terms` name, from `store`, as a verifier holds them; an
    /// error for terms of another release first, as its packages may not read in this one.
    pub fn packages(
        store: &PackageStore,
        terms: &SessionTerms,
    ) -> Result<ModePackages, StartError> {
        SessionRules::check_release(terms).map_err(StartError::Terms)?;
        ModePackages::from_store(store, terms.mode, &terms.map, &terms.dependencies)
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
        self.check_rules(&input)?;
        self.log
            .record_server(input, signature)
            .map_err(ServerInputRefused::Log)
    }

    /// Logs a server input as `record_server` does, signed in the log with the server's `key`;
    /// see `SessionLog::serve`.
    pub fn serve<C: Signing>(
        &mut self,
        input: ServerInput<'_>,
        secp: &Secp256k1<C>,
        key: &Keypair,
        aux: &[u8; 32],
    ) -> Result<(), ServerInputRefused> {
        self.check_rules(&input)?;
        self.log
            .serve(input, secp, key, aux)
            .map_err(ServerInputRefused::Log)
    }

    /// Refuses a change of a slot's controller that the mode's `[players]` does not allow.
    fn check_rules(&self, input: &ServerInput<'_>) -> Result<(), ServerInputRefused> {
        if let Some(change) = self.log.change_of(input).map_err(ServerInputRefused::Log)? {
            self.rules.check(change).map_err(ServerInputRefused::Rule)?;
        }
        Ok(())
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

    /// Logs the record of the checkpoint begun, with the server key's `signature` over it; see
    /// `SessionLog::record_checkpoint`.
    pub fn record_checkpoint(
        &mut self,
        record: Checkpoint,
        signature: &Signature,
    ) -> Result<(), CheckpointError> {
        self.log.record_checkpoint(record, signature)
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

    /// Publishes the log by adding the server seed of its last segment, which reveals every
    /// earlier one.
    pub fn reveal_seed(&mut self) {
        let last = self
            .seeds
            .seed(self.log.segment())
            .expect("the seeds reach the log's last segment");
        self.log.reveal_seed(last);
    }

    /// The session's log and seeds, its match's world left.
    pub(crate) fn into_parts(self) -> SessionParts {
        SessionParts {
            log: self.log,
            seeds: self.seeds,
        }
    }

    pub const fn log(&self) -> &SessionLog {
        &self.log
    }
}

#[cfg(feature = "bench")]
pub(crate) mod bench;
