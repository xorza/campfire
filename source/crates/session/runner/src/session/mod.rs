use std::num::NonZeroU64;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_capabilities::{Match, Release};
use campfire_common::{AfterLeave, Tick, Ticks};
use campfire_mode::{Mode, SavesData};
use campfire_package::ModePackages;
use campfire_protocol::{
    Applied, Checkpoint, CheckpointBegun, CheckpointError, InputError, LogLoadError, PlayerInput,
    RecordSink, ResultError, SeedError, ServerInput, ServerSeeds, SessionLog, SessionResult,
    Signature, SnapshotFingerprint,
};
use campfire_sim::{SimWorld, TickRate};
use secp256k1::{Keypair, Secp256k1, Signing};

use crate::match_build::MatchBuild;
use crate::session::error::{
    CheckpointBeginError, ResumeError, ServerInputRefused, SnapshotCheckError, StartError,
};
use crate::session_rules::SessionRules;
use crate::slot_rules::SlotRules;

pub(crate) mod error;
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
        let visibility = SessionRules::of(packages)
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
        let spec = MatchBuild::spec(packages, seed, rate, players).out_view(visibility.out());
        Release::build(world, spec);
        MatchBuild::install_mode(packages, world, players);
        let saves = packages.data().saves;
        let autosave = saves.autosave_ms.map(|ms| {
            rate.ticks(NonZeroU64::from(ms).get())
                .expect("a tick at least")
        });
        Ok(Session {
            seeds,
            segment,
            log,
            rules,
            saves,
            autosave,
        })
    }

    /// Restores `snapshot` in `world`, a match built for it, when it is the snapshot `record`,
    /// a checkpoint of the log, names, and checks that it restores to the state the record
    /// holds. An error for another snapshot, bytes that do not restore, and another state.
    pub(crate) fn restore_checked(
        world: &mut World,
        record: &Checkpoint,
        snapshot: &[u8],
    ) -> Result<(), SnapshotCheckError> {
        if SnapshotFingerprint::of(snapshot) != record.snapshot {
            return Err(SnapshotCheckError::Fingerprint);
        }
        world
            .restore(snapshot)
            .map_err(SnapshotCheckError::Restore)?;
        let restored = world.state_hash();
        if restored != record.state_hash {
            return Err(SnapshotCheckError::Hash {
                logged: record.state_hash,
                restored,
            });
        }
        Ok(())
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

    pub const fn log(&self) -> &SessionLog {
        &self.log
    }

    /// Prepares the world, a new one, for the match of `log`'s header, of the mode `packages`
    /// holds, at the header's tick rate and with the randomness of the first segment's seed of
    /// `seeds` and the players' contributions; starts the match; and inserts the session, which
    /// records into `log` from its first tick. An error when the terms name another release,
    /// mode or dependencies than this release and `packages`, a tick rate outside the mode's
    /// range, or more slots than the mode's teams have, when `seeds` are not the seeds of the
    /// chain the header commits to or do not reach the last segment of `log`, when a change of
    /// a slot's controller the log holds, as a published log replayed does, breaks the mode's
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

    /// Prepares the world, a new one, for the match of `log`, rewound to replay its ticks, from
    /// the checkpoint of it that starts `segment`, as a restore does: builds the match as
    /// `Session::start` does, but starts no mode, as the state is the checkpoint's `snapshot`;
    /// and seals the log's ticks before the checkpoint without running them, so the next tick
    /// the session runs is the checkpoint's. An error as `Session::start` gives one, and when the
    /// log holds no such checkpoint, when `snapshot` is not the one the checkpoint
    /// fingerprints, when it does not restore, or restores to another state hash than the
    /// checkpoint's.
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
            .ok_or(ResumeError::NoCheckpoint)?
            .clone();
        let mut session =
            Session::build(world, log, seeds, packages, segment).map_err(ResumeError::Start)?;
        Session::restore_checked(world, &record, snapshot).map_err(ResumeError::Snapshot)?;
        session.log.seal_until(record.tick);
        world.insert_resource(session);
        Ok(())
    }

    /// The match of `log` in a world of its own, as a verifier replays a log in: no network layer,
    /// and nothing but the session drives its ticks; started as `Session::start` starts it.
    pub fn start_match(
        log: SessionLog,
        seeds: ServerSeeds,
        packages: &ModePackages,
    ) -> Result<Match, StartError> {
        let mut world = World::new();
        Session::start(&mut world, log, seeds, packages)?;
        Ok(Match::from_built(world))
    }

    /// The match of `log` in a world of its own, resumed from the checkpoint that starts
    /// `segment`, as `Session::resume` resumes it.
    pub fn resume_match(
        log: SessionLog,
        seeds: ServerSeeds,
        packages: &ModePackages,
        segment: u32,
        snapshot: &[u8],
    ) -> Result<Match, ResumeError> {
        let mut world = World::new();
        Session::resume(&mut world, log, seeds, packages, segment, snapshot)?;
        Ok(Match::from_built(world))
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_capabilities::Match;
    use campfire_package::ModePackages;

    use crate::session::Session;
    use crate::session::error::ResumeError;

    impl Session {
        /// Loads into `game`, a session's match, the save that starts segment `segment`, whose
        /// snapshot is `snapshot`: the log goes back to it, as `SessionLog::load` says, and the
        /// match of the mode `packages` holds resumes from it, as `resume_match` does.
        pub fn load_save(
            mut game: Match,
            segment: u32,
            snapshot: &[u8],
            packages: &ModePackages,
        ) -> Result<Match, ResumeError> {
            let session = game
                .remove_resource::<Session>()
                .expect("a session's match holds its session");
            let (mut log, seeds) = (session.log, session.seeds);
            log.load(segment).map_err(ResumeError::Load)?;
            Session::resume_match(log.rewound(), seeds, packages, segment, snapshot)
        }
    }
}
