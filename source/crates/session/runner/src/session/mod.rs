use std::num::NonZeroU64;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_capabilities::Match;
use campfire_common::{AfterLeave, ErrorReport, PlayerSlot, Tick, Ticks};
use campfire_mode::{Mode, SavesData};
use campfire_package::ModePackages;
use campfire_players::PlayerSlotHolder;
use campfire_protocol::{
    Applied, Checkpoint, CheckpointBegun, CheckpointError, Controller, Delegation, InputChain,
    InputError, Journal, LogLoadError, LogReplay, PlayerInput, PlayerPacket, PlayerSlotChangeKind,
    PlayerSlotStart, RecordSink, ResultError, SeedError, ServerInput, ServerInputError, ServerSeed,
    ServerSeeds, SessionLog, SessionResult, Signer, SnapshotFingerprint, Taken,
};
use campfire_sim::{MatchRate, SimRng, SimWorld, TickInput, TickInputs};

use crate::match_build::MatchBuild;
use crate::player_slot_rules::PlayerSlotRules;
use crate::session::error::{
    BotInputDrop, CheckpointBeginError, ResumeError, ServerInputRefusal, SnapshotCheckError,
    StartError,
};
use crate::session::session_source::{ResumeAt, SessionSource};
use crate::session_rules::{CheckedTerms, SessionRules};

pub(crate) mod error;
pub(crate) mod session_source;
pub(crate) mod session_world;

/// A match's session log and state types, kept as a resource in the `World` that runs the match: a
/// bare one on a verifier, the network stack's on a server. The server records inputs as they
/// arrive, and signs what it logs with the signer the session holds; a verifier records a published
/// log's inputs again, and its session holds no signer. Either way each tick applies exactly the
/// inputs the log gives it, and draws from the seed of the segment it runs in.
#[derive(Resource, Debug)]
pub struct Session {
    /// The server seeds of the segments the session may run, secret until `reveal_seed`
    /// publishes the log.
    seeds: ServerSeeds,
    /// The segment whose seed the sim draws from.
    segment: u32,
    log: SessionLog,
    /// Who the mode lets take a player slot.
    rules: PlayerSlotRules,
    /// Who may ask for a save, and the mode's autosaves.
    saves: SavesData,
    /// The ticks between two autosaves, without which the mode makes none.
    autosave: Option<Ticks>,
    /// A server's, with its signer and journal, or a verifier's.
    role: SessionRole,
    /// The replay of the log's sealed ticks, while the session runs them again: a verifier's,
    /// a restore's or a load's; none once it reached the log's next tick
    /// (08-sessions.md, The replay is a cursor the session owns).
    replay: Option<LogReplay>,
    /// The log's terms as the mode's rules checked them, once for the session.
    checked: CheckedTerms,
}

/// What a server's session signs and keeps with: its signer, and its journal once it keeps one.
#[derive(Debug)]
struct ServerRole<'r> {
    signer: &'r Signer,
    journal: Option<&'r mut Journal>,
}

/// What a session is to its host: a server's, which signs what it logs with its signer and keeps
/// a journal of its log once its server keeps one (08-sessions.md, The journal belongs to the
/// server's session); or a verifier's, which records a published log's inputs again and signs
/// and journals nothing.
#[derive(Debug)]
enum SessionRole {
    Server {
        signer: Arc<Signer>,
        journal: Option<Journal>,
    },
    Verifier,
}

impl SessionRole {
    /// The server's of `signer`, or a verifier's.
    fn of(signer: Option<Arc<Signer>>) -> Self {
        match signer {
            Some(signer) => Self::Server {
                signer,
                journal: None,
            },
            None => Self::Verifier,
        }
    }

    const fn signer(&self) -> Option<&Arc<Signer>> {
        match self {
            Self::Server { signer, .. } => Some(signer),
            Self::Verifier => None,
        }
    }

    const fn journal(&self) -> Option<&Journal> {
        match self {
            Self::Server { journal, .. } => journal.as_ref(),
            Self::Verifier => None,
        }
    }

    const fn journal_mut(&mut self) -> Option<&mut Journal> {
        match self {
            Self::Server { journal, .. } => journal.as_mut(),
            Self::Verifier => None,
        }
    }

    /// The server's signer and journal, which only a server's session signs and keeps with: a
    /// verifier's that serves or records is a logic error.
    fn server(&mut self) -> ServerRole<'_> {
        match self {
            Self::Server { signer, journal } => ServerRole {
                signer,
                journal: journal.as_mut(),
            },
            Self::Verifier => panic!("only a server's session serves and records"),
        }
    }

    /// The journal place of a server's session, which keeps one journal.
    fn new_journal(&mut self) -> &mut Option<Journal> {
        match self {
            Self::Server { journal, .. } => {
                assert!(journal.is_none(), "a session keeps one journal");
                journal
            }
            Self::Verifier => panic!("a verifier's session keeps no journal"),
        }
    }
}

/// Where the player of a delegation sits as they join, as `Session::player_slot_for` finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerSlotFor {
    /// The player slot their main key holds, with its chain as logged; `renews` when the delegation
    /// names another session key than the player slot's, which the player takes back by a renewal.
    Held {
        player_slot: PlayerSlot,
        chain: InputChain,
        renews: bool,
    },
    /// A player slot they join: the one they left, else an open one, else a bot's, as the mode's
    /// `[players]` admits.
    Join(PlayerSlot),
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
            checked,
            signer,
        } = source;
        let header = log.header();
        debug_assert_eq!(
            SessionRules::of(packages).check(&header.terms).as_ref(),
            Ok(&checked),
            "a session's source holds its log's terms checked"
        );
        let CheckedTerms { visibility, picks } = &checked;
        if seeds.last() < log.segment() {
            return Err(StartError::Seed(SeedError::NotRevealed));
        }
        if !seeds.commits_to(&header.terms.seed_commitment) {
            return Err(StartError::Seed(SeedError::WrongSeed));
        }
        let server_seed = seeds
            .seed(segment)
            .expect("the seeds reach the log's last segment");
        let seed = header.segment_seed(segment, server_seed);
        let rules = PlayerSlotRules::new(packages.data().players);
        for &change in log.changes() {
            rules
                .check(change.kind)
                .map_err(|error| StartError::PlayerSlotRule { change, error })?;
        }
        let players = log.player_slot_count();
        let rate = MatchRate::new(header.terms.tick_hz);
        let spec = MatchBuild::spec(packages, seed, rate, players).visibility(*visibility);
        MatchBuild::build(world, spec, packages, players, picks);
        let saves = packages.data().saves;
        let autosave = saves.autosave_ms.map(|ms| {
            rate.ticks(NonZeroU64::from(ms).get())
                .expect("a tick at least")
        });
        Ok(Self {
            seeds,
            segment,
            rules,
            saves,
            autosave,
            role: SessionRole::of(signer),
            replay: LogReplay::of(&log),
            log,
            checked,
        })
    }

    /// The tick the session runs next: its replay's, while one runs, else the log's.
    pub fn next_tick(&self) -> Tick {
        self.replay
            .as_ref()
            .map_or_else(|| self.log.next_tick(), LogReplay::next_tick)
    }

    /// Whether the session runs another tick: one its replay holds, or a live one, which a
    /// server's session alone runs; a verifier's stops at its log's end, as a local server's
    /// replay of a report does (08-sessions.md, The client opens a report).
    pub const fn runs_on(&self) -> bool {
        self.replay.is_some() || matches!(self.role, SessionRole::Server { .. })
    }

    /// Drops the replay once it reached the log's next tick, from where the session runs live.
    pub(crate) fn end_replay(&mut self) {
        if self
            .replay
            .as_ref()
            .is_some_and(|replay| replay.is_done(&self.log))
        {
            self.replay = None;
        }
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
        Self::restore_hash_checked(world, record, snapshot)
    }

    /// Restores `snapshot` in `world` as `restore_checked` does, once its caller checked its
    /// fingerprint, as a load checks a save's before it changes the log.
    pub(crate) fn restore_hash_checked(
        world: &mut World,
        record: &Checkpoint,
        snapshot: &[u8],
    ) -> Result<(), SnapshotCheckError> {
        let restored = world
            .restore(snapshot)
            .map_err(SnapshotCheckError::Restore)?;
        if restored != record.state_hash {
            return Err(SnapshotCheckError::Hash {
                logged: record.state_hash,
                restored,
            });
        }
        Ok(())
    }

    /// The match of its packages `packages` to restore a snapshot of its log into, of the log's
    /// player slots and tick rate and the picks its terms hold, as `MatchBuild::restore_target`
    /// builds one.
    pub fn restore_target(&self, packages: &ModePackages) -> Match {
        let rate = MatchRate::new(self.log.header().terms.tick_hz);
        let picks = &self.checked.picks;
        MatchBuild::restore_target(packages, rate, self.log.player_slot_count(), picks)
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
        self.log.record(packet, applied, self.role.journal_mut())
    }

    /// Where the player of `delegation` sits as they join: the player slot their main key holds;
    /// else the player slot they left, unless another player holds it; else the first open player
    /// slot, with late join; else the first bot's, with bot takeover; none when no player slot is
    /// theirs or open to them.
    pub fn player_slot_for(&self, delegation: &Delegation) -> Option<PlayerSlotFor> {
        let main = delegation.main_key();
        let log = &self.log;
        let player_slots = (0..log.player_slot_count()).map(PlayerSlot::new);
        let held =
            player_slots
                .clone()
                .find_map(|player_slot| match log.controller(player_slot)? {
                    Controller::Player {
                        delegation: holder,
                        chain,
                    } if holder.main_key() == main => Some(PlayerSlotFor::Held {
                        player_slot,
                        chain,
                        renews: holder.terms().session_key != delegation.terms().session_key,
                    }),
                    _ => None,
                });
        if held.is_some() {
            return held;
        }
        // A player slot they left that another player took since is not theirs.
        let left = player_slots.clone().find(|&player_slot| {
            log.leaver(player_slot) == Some(*main)
                && !matches!(log.controller(player_slot), Some(Controller::Player { .. }))
        });
        let joinable = |from: Taken| {
            self.rules
                .check(PlayerSlotChangeKind::Joined { from })
                .is_ok()
        };
        let free = |wanted: Controller<'_>| {
            player_slots
                .clone()
                .find(|&player_slot| log.controller(player_slot) == Some(wanted))
        };
        let open = joinable(Taken::Open)
            .then(|| free(Controller::Open))
            .flatten();
        let bot = || {
            joinable(Taken::Bot)
                .then(|| free(Controller::Bot))
                .flatten()
        };
        left.or(open).or_else(bot).map(PlayerSlotFor::Join)
    }

    /// Logs a server input before the next tick, signed in the log by the server at the place it
    /// takes; see `SessionLog::serve`. The mode's `[players]` refuses a change of a player slot's
    /// controller it does not allow, before the log takes it.
    pub fn serve(&mut self, input: ServerInput<'_>) -> Result<(), ServerInputRefusal> {
        let serving = self.log.serving(input).map_err(ServerInputRefusal::Log)?;
        if let Some(change) = serving.change() {
            self.rules.check(change).map_err(ServerInputRefusal::Rule)?;
        }
        let ServerRole { signer, journal } = self.role.server();
        serving
            .log(signer, journal)
            .map_err(ServerInputRefusal::Log)
    }

    /// Logs `payload` as an input of the bot of `player_slot`, as `serve` logs a server input; the
    /// cause the log refused it for, which drops it. A bot's input holds every other rule of the
    /// log, so any other refusal is a logic error.
    pub fn serve_bot(
        &mut self,
        player_slot: PlayerSlot,
        payload: &[u8],
    ) -> Result<(), BotInputDrop> {
        let input = ServerInput::Bot {
            player_slot,
            payload,
        };
        match self.serve(input) {
            Ok(()) => Ok(()),
            Err(ServerInputRefusal::Log(ServerInputError::AheadOfTime)) => {
                Err(BotInputDrop::AheadOfTime)
            }
            Err(ServerInputRefusal::Log(ServerInputError::LogFull)) => Err(BotInputDrop::LogFull),
            Err(error) => panic!("the server's bot input holds: {}", ErrorReport::of(&error)),
        }
    }

    /// The server's signer, which signs what the session logs; none on a verifier.
    pub const fn signer(&self) -> Option<&Arc<Signer>> {
        self.role.signer()
    }

    /// Goes back to the save that starts segment `segment`; see `SessionLog::load`. The match's
    /// world is then stale: a new one resumes from the save.
    pub fn load(&mut self, segment: u32) -> Result<(), LogLoadError> {
        self.log.load(segment, self.role.journal_mut())
    }

    /// Who may ask for a save, and the mode's autosaves, as its `[saves]` says.
    pub const fn saves(&self) -> SavesData {
        self.saves
    }

    /// What the player slot of a player who leaves becomes, as the mode's `[players] leaver` says.
    pub const fn after_leave(&self) -> AfterLeave {
        self.rules.after_leave()
    }

    /// Keeps `sink` as the journal of the session's log, a new one, given the records of what
    /// the log holds: see `Journal::start`.
    pub fn keep_journal(&mut self, sink: Box<dyn RecordSink>) {
        *self.role.new_journal() = Some(Journal::start(&self.log, sink));
    }

    /// Keeps `sink` as the journal of the session's log, of `len` bytes, which holds every
    /// record of it; see `Journal::resume`.
    pub fn resume_journal(&mut self, sink: Box<dyn RecordSink>, len: u64) {
        *self.role.new_journal() = Some(Journal::resume(&self.log, sink, len));
    }

    /// Starts drawing the sim's random numbers from `segment`'s seed in `world`, the segment the
    /// session stands in from now on, as a tick crosses into it or a save of it loads.
    pub(crate) fn draw_from(&mut self, world: &mut World, segment: u32) {
        let server_seed = self
            .seeds
            .seed(segment)
            .expect("the seeds reach the log's last segment");
        let seed = self.log.header().segment_seed(segment, server_seed);
        world.insert_resource(SimRng::new(seed));
        self.segment = segment;
    }

    /// Pushes each of `sealed`, a tick's inputs, into `inputs`, which the tick reads.
    pub(crate) fn push_inputs<'l>(
        inputs: &mut TickInputs,
        sealed: impl IntoIterator<Item = PlayerInput<'l>>,
    ) {
        for input in sealed {
            inputs.push(TickInput {
                player_slot: input.player_slot,
                payload: input.payload,
            });
        }
    }

    /// The server seed of the log's last segment, which a published log reveals: at the session's
    /// end, or as a local server publishes the log so far into a player's report
    /// (08-sessions.md, The log is the local server's, published at the report's tick).
    pub fn last_seed(&self) -> &ServerSeed {
        self.seeds
            .seed(self.log.segment())
            .expect("the seeds reach the log's last segment")
    }

    /// The journal of the session's log, once the server keeps one.
    pub const fn journal(&self) -> Option<&Journal> {
        self.role.journal()
    }

    /// The journal of the session's log, once the server keeps one.
    pub const fn journal_mut(&mut self) -> Option<&mut Journal> {
        self.role.journal_mut()
    }

    /// Closes the journal of the session's log, its drop handing its sink what it holds before
    /// the sink ends, as a session that ends does before its log is published: the session keeps
    /// no journal after.
    pub fn close_journal(&mut self) {
        if let SessionRole::Server { journal, .. } = &mut self.role {
            *journal = None;
        }
    }

    /// Whether the log may refuse its next entry, its journal's room counted; see
    /// `SessionLog::is_full`.
    pub fn is_log_full(&self) -> bool {
        self.log.is_full(self.role.journal())
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
            .begin_checkpoint(self.role.journal_mut())
            .map_err(CheckpointBeginError::Log)
    }

    /// Logs the record of the checkpoint begun, which the server signs; see
    /// `SessionLog::serve_checkpoint`.
    pub fn serve_checkpoint(&mut self, record: Checkpoint) -> Result<(), CheckpointError> {
        let ServerRole { signer, journal } = self.role.server();
        self.log.serve_checkpoint(record, signer, journal)
    }

    /// Logs the result, which the server signs; see `SessionLog::serve_result`.
    pub fn serve_result(&mut self, result: SessionResult) -> Result<(), ResultError> {
        let ServerRole { signer, journal } = self.role.server();
        self.log.serve_result(result, signer, journal)
    }

    /// Publishes the log by adding the server seed of its last segment, which reveals every
    /// earlier one.
    pub fn reveal_seed(&mut self) {
        self.log.reveal_seed(self.last_seed().clone());
    }

    pub const fn log(&self) -> &SessionLog {
        &self.log
    }

    /// Whether a player or a bot held each player slot of the session, in order, as the match
    /// started: as its log's header gives it, whatever a later change made of the player slot, so a
    /// replay of a published log starts as the run did.
    fn holders(&self) -> Vec<PlayerSlotHolder> {
        let starts = self.log.header().player_slots.iter();
        starts
            .map(|start| match start {
                PlayerSlotStart::Player(_) | PlayerSlotStart::Bot => PlayerSlotHolder::Held,
                PlayerSlotStart::Open => PlayerSlotHolder::Open,
            })
            .collect()
    }

    /// Prepares the world, a new one, for the match of `source`'s log's header, of the mode its
    /// packages hold, at the header's tick rate and with the randomness of the first segment's seed
    /// of its seeds and the players' contributions; starts the match; and inserts the session,
    /// which records into the log from its first tick. An error when the terms name another
    /// release, mode or dependencies than this release and the packages, a tick rate outside the
    /// mode's range, or more player slots than the mode's teams have, when the seeds are not those
    /// of the chain the header commits to or do not reach the log's last segment, when a change of
    /// a player slot's controller the log holds, as a published log replayed does, breaks the
    /// mode's `[players]`, when the mode's `on_match_start` fails, or when the log's file starts at
    /// a checkpoint, whose snapshot a resume starts from.
    pub fn start(world: &mut World, source: SessionSource<'_>) -> Result<(), StartError> {
        if let Some(record) = source.log.starts_at() {
            return Err(StartError::AtCheckpoint {
                segment: record.segment,
            });
        }
        let session = Self::build(world, source, 0)?;
        assert_eq!(
            session.next_tick(),
            Tick::new(0),
            "a session starts before its first tick"
        );
        Mode::start(world, &session.holders()).map_err(StartError::MatchStart)?;
        world.insert_resource(session);
        Ok(())
    }

    /// Prepares the world, a new one, for the match of `source`, its log's ticks to replay, from
    /// the checkpoint `at`, as a restore does: builds the match as `Session::start` does, but
    /// starts no mode, as the state is the checkpoint's snapshot; and seals the log's ticks before
    /// the checkpoint without running them, so the next tick the session runs is the
    /// checkpoint's. An error as `Session::start` gives one, and when the log holds no such
    /// checkpoint, when the snapshot is not the one the checkpoint fingerprints, when it does not
    /// restore, or restores to another state hash than the checkpoint's.
    pub fn resume(
        world: &mut World,
        source: SessionSource<'_>,
        at: ResumeAt<'_>,
    ) -> Result<(), ResumeError> {
        let record = source
            .log
            .checkpoints()
            .find(|record| record.segment == at.segment)
            .ok_or(ResumeError::NoCheckpoint)?
            .clone();
        let mut session = Self::build(world, source, at.segment).map_err(ResumeError::Start)?;
        Self::restore_checked(world, &record, at.snapshot).map_err(ResumeError::Snapshot)?;
        if let Some(replay) = &mut session.replay {
            replay.seal_until(&session.log, record.tick);
        }
        session.end_replay();
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
        /// Loads the save `at` of `game`, a session's match of the mode `packages` holds, into a
        /// match built fresh: the log goes back to it, as `SessionLog::load` says, and the match
        /// resumes from it, as `resume_match` does; what a load in place must match.
        pub fn resume_save(
            mut game: Match,
            packages: &ModePackages,
            at: ResumeAt<'_>,
        ) -> Result<Match, ResumeError> {
            let session = game
                .remove_resource::<Self>()
                .expect("a session's match holds its session");
            let Self {
                mut log,
                seeds,
                role,
                checked,
                ..
            } = session;
            let signer = role.signer().cloned();
            log.load(at.segment, None).map_err(ResumeError::Load)?;
            let source = SessionSource {
                log,
                seeds,
                packages,
                checked,
                signer,
            };
            Self::resume_match(source, at)
        }

        /// Fills the session's log, as `SessionLog::fill` does.
        pub fn fill_log(&mut self) {
            self.log.fill();
        }
    }
}
