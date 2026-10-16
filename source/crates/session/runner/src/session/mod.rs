use std::num::NonZeroU64;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_capabilities::{Match, Release};
use campfire_common::{AfterLeave, Tick, Ticks};
use campfire_mode::{Mode, SavesData};
use campfire_players::Seating;
use campfire_protocol::{
    Applied, Checkpoint, CheckpointBegun, CheckpointError, InputError, LogLoadError, PlayerPacket,
    RecordSink, ResultError, SeedError, ServerInput, ServerSeeds, SessionLog, SessionResult,
    Signer, SlotStart, SnapshotFingerprint,
};
use campfire_sim::{SimWorld, TickRate};

use crate::match_build::MatchBuild;
use crate::session::error::{
    CheckpointBeginError, ResumeError, ServerInputRefused, SnapshotCheckError, StartError,
};
use crate::session::session_source::{ResumeAt, SessionSource};
use crate::session_rules::SessionRules;
use crate::slot_rules::SlotRules;

pub(crate) mod error;
pub(crate) mod session_source;
pub(crate) mod session_world;

/// A match's session log and state types, kept as a resource in the `World` that runs the match:
/// a bare one on a verifier, Lightyear's on a server. The server records inputs as they arrive,
/// and signs what it logs with the signer it holds apart; a verifier records a published log's
/// inputs again, and holds no signer. Either way each tick applies exactly the inputs the log
/// gives it, and draws from the seed of the segment it runs in.
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
    /// Builds the match of `source`'s log's header in `world`, of the mode its packages hold, at
    /// the header's tick rate and with the randomness of `segment`'s seed; the session of the
    /// log, drawing from `segment`. An error as `start` gives one, but for the mode's start.
    fn build(
        world: &mut World,
        source: SessionSource<'_>,
        segment: u32,
    ) -> Result<Self, StartError> {
        let SessionSource {
            log,
            seeds,
            packages,
        } = source;
        let header = log.header();
        let visibility = SessionRules::of(packages)
            .check(&header.terms)
            .map_err(StartError::Terms)?;
        if seeds.last() < log.segment() {
            return Err(StartError::Seed(SeedError::NotRevealed));
        }
        if !seeds.commits_to(&header.terms.seed_commitment) {
            return Err(StartError::Seed(SeedError::WrongSeed));
        }
        let server_seed = seeds
            .seed(segment)
            .expect("the seeds reach the log's last segment");
        let seed = header.segment_seed(segment, &server_seed);
        let rules = SlotRules::new(packages.data().players);
        for &change in log.changes() {
            rules
                .check(change.kind)
                .map_err(|error| StartError::SlotRule { change, error })?;
        }
        let players = u32::try_from(header.slots.len()).expect("the log counts slots in u32");
        let rate = TickRate::new(header.terms.tick_hz);
        let spec = MatchBuild::spec(packages, seed, rate, players).out(visibility.out());
        Release::build(world, spec);
        MatchBuild::install_mode(world, packages, players);
        let saves = packages.data().saves;
        let autosave = saves.autosave_ms.map(|ms| {
            rate.ticks(NonZeroU64::from(ms).get())
                .expect("a tick at least")
        });
        Ok(Self {
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
            return Err(SnapshotCheckError::PackageFingerprint);
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
        packet: PlayerPacket<'_, I>,
        applied: &mut Vec<Applied>,
    ) -> Result<(), InputError>
    where
        I: IntoIterator<Item = (Tick, &'a [u8])>,
        I::IntoIter: Clone,
    {
        self.log.record(packet, applied)
    }

    /// Logs a server input before the next tick, signed in the log by `server` at the place it
    /// takes; see `SessionLog::serve`. The mode's `[players]` refuses a change of a slot's
    /// controller it does not allow, before the log takes it.
    pub fn serve(
        &mut self,
        input: ServerInput<'_>,
        server: &Signer,
    ) -> Result<(), ServerInputRefused> {
        self.check_rules(&input)?;
        self.log
            .serve(input, server)
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

    /// Keeps `sink` as the journal of the session's log, of `len` bytes, which holds every
    /// record of it; see `SessionLog::resume_journal`.
    pub fn resume_journal(&mut self, sink: Box<dyn RecordSink>, len: u64) {
        self.log.resume_journal(sink, len);
    }

    /// Closes the journal of the session's log: see `SessionLog::close_journal`.
    pub fn close_journal(&mut self) {
        self.log.close_journal();
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

    /// Logs the record of the checkpoint begun, signed in the log by `server`; see
    /// `SessionLog::serve_checkpoint`.
    pub fn record_checkpoint(
        &mut self,
        record: Checkpoint,
        server: &Signer,
    ) -> Result<(), CheckpointError> {
        self.log.serve_checkpoint(record, server)
    }

    /// Logs the result, signed in the log by `server`; see `SessionLog::serve_result`.
    pub fn record_result(
        &mut self,
        result: SessionResult,
        server: &Signer,
    ) -> Result<(), ResultError> {
        self.log.serve_result(result, server)
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

    /// Whether a player or a bot held each slot of the session, in order, as the match started:
    /// as its log's header gives it, whatever a later change made of the slot, so a replay of a
    /// published log starts as the run did.
    fn seatings(&self) -> Vec<Seating> {
        let starts = self.log.header().slots.iter();
        starts
            .map(|start| match start {
                SlotStart::Player(_) | SlotStart::Bot => Seating::Held,
                SlotStart::Open => Seating::Open,
            })
            .collect()
    }

    /// Prepares the world, a new one, for the match of `source`'s log's header, of the mode its
    /// packages hold, at the header's tick rate and with the randomness of the first segment's
    /// seed of its seeds and the players' contributions; starts the match; and inserts the
    /// session, which records into the log from its first tick. An error when the terms name
    /// another release, mode or dependencies than this release and the packages, a tick rate
    /// outside the mode's range, or more slots than the mode's teams have, when the seeds are not
    /// those of the chain the header commits to or do not reach the log's last segment, when a
    /// change of a slot's controller the log holds, as a published log replayed does, breaks the
    /// mode's `[players]`, or when the packages do not load into the match.
    pub fn start(world: &mut World, source: SessionSource<'_>) -> Result<(), StartError> {
        assert_eq!(
            source.log.next_tick(),
            Tick::new(0),
            "a session starts before its first tick"
        );
        let session = Self::build(world, source, 0)?;
        Mode::start(world, &session.seatings()).map_err(StartError::MatchStart)?;
        world.insert_resource(session);
        Ok(())
    }

    /// Prepares the world, a new one, for the match of `source`, its log rewound to replay its
    /// ticks, from the checkpoint `at`, as a restore does: builds the match as `Session::start`
    /// does, but starts no mode, as the state is the checkpoint's snapshot; and seals the log's
    /// ticks before the checkpoint without running them, so the next tick the session runs is
    /// the checkpoint's. An error as `Session::start` gives one, and when the log holds no such
    /// checkpoint, when the snapshot is not the one the checkpoint fingerprints, when it does not
    /// restore, or restores to another state hash than the checkpoint's.
    pub fn resume(
        world: &mut World,
        source: SessionSource<'_>,
        at: ResumeAt<'_>,
    ) -> Result<(), ResumeError> {
        assert_eq!(
            source.log.next_tick(),
            Tick::new(0),
            "a resumed log replays from its first tick"
        );
        let record = source
            .log
            .checkpoints()
            .find(|record| record.segment == at.segment)
            .ok_or(ResumeError::NoCheckpoint)?
            .clone();
        let mut session = Self::build(world, source, at.segment).map_err(ResumeError::Start)?;
        Self::restore_checked(world, &record, at.snapshot).map_err(ResumeError::Snapshot)?;
        session.log.seal_until(record.tick);
        world.insert_resource(session);
        Ok(())
    }

    /// The match of `source` in a world of its own, as a verifier replays a log in: no network
    /// layer, and nothing but the session drives its ticks; started as `Session::start` starts
    /// it.
    pub fn start_match(source: SessionSource<'_>) -> Result<Match, StartError> {
        let mut world = World::new();
        Self::start(&mut world, source)?;
        Ok(Match::from_built(world))
    }

    /// The match of `source` in a world of its own, resumed from the checkpoint `at`, as
    /// `Session::resume` resumes it.
    pub fn resume_match(source: SessionSource<'_>, at: ResumeAt<'_>) -> Result<Match, ResumeError> {
        let mut world = World::new();
        Self::resume(&mut world, source, at)?;
        Ok(Match::from_built(world))
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_capabilities::Match;
    use campfire_package::ModePackages;

    use crate::session::Session;
    use crate::session::error::ResumeError;
    use crate::session::session_source::{ResumeAt, SessionSource};

    impl Session {
        /// Loads into `game`, a session's match of the mode `packages` holds, the save `at`: the
        /// log goes back to it, as `SessionLog::load` says, and the match resumes from it, as
        /// `resume_match` does.
        pub fn load_save(
            mut game: Match,
            packages: &ModePackages,
            at: ResumeAt<'_>,
        ) -> Result<Match, ResumeError> {
            let session = game
                .remove_resource::<Self>()
                .expect("a session's match holds its session");
            let Self { mut log, seeds, .. } = session;
            log.load(at.segment).map_err(ResumeError::Load)?;
            let source = SessionSource {
                log: log.rewound(),
                seeds,
                packages,
            };
            Self::resume_match(source, at)
        }

        /// Fills the session's log, as `SessionLog::fill` does.
        pub fn fill_log(&mut self) {
            self.log.fill();
        }
    }
}
