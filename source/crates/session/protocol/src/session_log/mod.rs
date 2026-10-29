use std::iter;
use std::ops::Range;

use campfire_codec::{Binary, BinaryError};
use campfire_common::{PlayerSlot, Tick};
use serde::{Deserialize, Serialize};

use crate::checkpoint::Checkpoint;
use crate::checkpoint::checkpoint_begun::CheckpointBegun;
use crate::checkpoint::log_carry::{CarriedInput, CarriedPlayerSlot, LogCarry};
use crate::control::{Control, Controller};
use crate::delegation::Delegation;
use crate::input_chain::InputChain;
use crate::main_key::MainKey;
use crate::player_input::PlayerInput;
use crate::player_slot_change::{PlayerSlotChange, PlayerSlotChangeKind, Taken};
use crate::player_slot_start::PlayerSlotStart;
use crate::server_input::{EncodedPlace, InputPlace, PlacedInput, ServerInput};
use crate::server_seed::ServerSeed;
use crate::server_seeds::ServerSeeds;
use crate::session_id::SessionId;
use crate::session_log::applied::Applied;
use crate::session_log::checked_carry::{CarryCx, CheckedCarry};
use crate::session_log::durable_head::DurableHead;
use crate::session_log::error::{
    CarryFlaw, CheckpointError, HeaderError, InputError, LogError, LogLoadError, ResultError,
    ServerInputError,
};
use crate::session_log::input_schedule::{InputOrder, InputSchedule};
use crate::session_log::journal::Journal;
use crate::session_log::journal::journal_frames::{self, MAX_RECORD};
use crate::session_log::log_entries::{LogEntries, Served};
use crate::session_log::log_entry::{HeldStart, LogEntry};
use crate::session_log::log_replay::LogReplay;
use crate::session_log::player_packet::PlayerPacket;
use crate::session_log::player_slot_control::{PlayerSlotControl, PlayerSlotState, TakenPacket};
use crate::session_log::segments::{Boundary, LoggedCheckpoint, SegmentStart, Segments};
use crate::session_log::session_header::SessionHeader;
use crate::session_log::signature_checks::SignatureChecks;
use crate::session_result::SessionResult;
use crate::signature::Signature;
use crate::signed::InSession;
use crate::signer::Signer;

pub(crate) mod applied;
mod checked_carry;
pub(crate) mod durable_head;
pub(crate) mod durable_heads;
pub(crate) mod error;
mod file;
mod input_schedule;
pub(crate) mod journal;
mod journal_kinds;
mod journal_records;
pub(crate) mod journal_replay;
pub(crate) mod journal_shape;
mod log_entries;
pub(crate) mod log_entry;
pub(crate) mod log_replay;
pub(crate) mod paged_bytes;
pub(crate) mod paged_vec;
pub(crate) mod player_packet;
mod player_slot_control;
pub(crate) mod publish;
mod segments;
pub(crate) mod session_header;
mod signature_checks;

/// What one record adds beside its own bytes: in the journal its frame and its kind's byte, in
/// the file its kind's byte and the growth of the counts before it, each at most 10 bytes.
const RECORD_SLACK: u64 = 64;
/// The records the session's end may take past the room the last entry leaves, each of at most
/// a record's bytes: the begin of a checkpoint and its record, the seal of the tick before the
/// end, and the result with the revealed seed.
const END_RECORDS: u64 = 4;

/// The entries of a session, in memory, in the order they were logged, and grouped by the tick that
/// was next when each arrived: the players' packets and the server's inputs. The applied tick
/// follows from that group, so a log cannot hold a wrong one. A player's inputs arrive in packets,
/// each signed once over the player's chain head after it, and each server input is signed over its
/// place, so every logged entry is signed. The log follows who controls each player slot, a player,
/// a bot, or none, from the server's inputs, and takes a player's packet only for a player slot the
/// player controls. A checkpoint ends a segment and starts the next, whose seed the sim draws from,
/// and a result ends the session. The server records entries as they arrive; decoding a published
/// log records them again, which checks every chain link and signature, and a verifier then replays
/// the decoded log's ticks through a `LogReplay`.
#[derive(Debug)]
pub struct SessionLog {
    header: SessionHeader,
    /// Present once the segment is published.
    revealed: Option<ServerSeed>,
    checks: SignatureChecks,
    /// The scratch a record's signed message is written into.
    message: Vec<u8>,
    /// The record the log takes now, its kind's byte then its body, encoded once: what its
    /// file's count measures, what a server input's signature covers, and what its journal
    /// frames (08-sessions.md, A record is encoded once).
    record: Vec<u8>,
    /// What the log holds, as it was logged.
    entries: LogEntries,
    /// Who controls each player slot.
    control: PlayerSlotControl,
    /// When each input applies.
    schedule: InputSchedule,
    segments: Segments,
    /// Whether the log went back to a save: a load it took, or one in the journal it was
    /// rebuilt from.
    loaded: bool,
    /// The segment from whose boundary the log holds its entries, once it forgot those before;
    /// none while it holds every entry it took.
    base: Option<u32>,
    /// How the session ended, once it did.
    result: Option<LoggedResult>,
    /// The server inputs logged since the last tick was sealed.
    server_since: u32,
    /// Every change of a player slot's controller, in the order logged, so by tick.
    changes: Vec<PlayerSlotChange>,
    /// The changes of the tick last sealed.
    sealed_changes: Range<usize>,
    /// The most bytes the log's file and its journal hold.
    bounds: Bounds,
    /// The bytes `encode` writes now.
    file_len: u64,
    /// The most bytes one record adds to the file or the journal: the terms' largest record and
    /// `RECORD_SLACK`.
    unit: u64,
    /// The hash of the header's terms.
    session_id: SessionId,
}

/// A server input the log checked and has yet to log, with what it does to its player slot's
/// controller, which a caller's rule may refuse first, as the session's `[players]` does.
#[derive(Debug)]
pub struct ServerServe<'l, 'i> {
    log: &'l mut SessionLog,
    input: ServerInput<'i>,
    change: Option<PlayerSlotChangeKind>,
}

impl ServerServe<'_, '_> {
    /// What the input does to its player slot's controller, if anything.
    pub const fn change(&self) -> Option<PlayerSlotChangeKind> {
        self.change
    }

    /// Logs the input, signed by `server`, of the terms' server key, when the file and `journal`
    /// have room for it. A signature the log made over the message it wrote needs no check, which
    /// is the most a server input costs.
    pub fn log(
        self,
        server: &Signer,
        journal: Option<&mut Journal>,
    ) -> Result<(), ServerInputError> {
        let Self { log, input, change } = self;
        log.debug_server(server);
        let placed = EncodedPlace {
            session_id: log.session_id,
            place: log.next_place(),
            input: &log.record[1..],
        };
        let signature = server.sign_with(&placed, &mut log.message);
        debug_assert!(
            {
                let whole = PlacedInput {
                    input: &input,
                    session_id: log.session_id,
                    place: log.next_place(),
                };
                let server_key = log.header.terms.server_key;
                log.checks.verifies(&signature, &server_key, &whole)
            },
            "a signature the log made over a server input's encoding is one over the input"
        );
        log.log_server(input, change, &signature, journal)
    }
}

/// An input the log applied: its player slot, its stamp, and the tick it took effect in, a
/// server bot's among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppliedInput {
    pub player_slot: PlayerSlot,
    pub stamp: Tick,
    pub tick: Tick,
}

/// The most bytes a log's file and its journal hold: `MAX_FILE_LEN` and `MAX_JOURNAL_LEN`, which
/// a build of `internals` lets a test lower, and which a build of none holds as no field.
#[cfg(feature = "internals")]
#[derive(Debug, Clone, Copy)]
struct Bounds {
    file: u64,
    journal: u64,
}

#[cfg(not(feature = "internals"))]
#[derive(Debug, Clone, Copy)]
struct Bounds;

/// The bytes a record adds to the log's file and to its journal.
#[derive(Debug, Clone, Copy)]
struct Growth {
    file: u64,
    journal: u64,
}

/// Where the past before a checkpoint's boundary is held once its record comes: in a journal, a
/// server's or one the log is rebuilt from, so the log forgets it, or in the log alone, which
/// keeps it (08-sessions.md, The live log is its latest checkpoint's state and the entries
/// since).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Past {
    InJournal,
    InLog,
}

/// The result and the server's signature over it, as the file holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct LoggedResult {
    result: SessionResult,
    signature: Signature,
}

#[cfg(feature = "internals")]
impl Bounds {
    const MAX: Self = Self {
        file: SessionLog::MAX_FILE_LEN as u64,
        journal: SessionLog::MAX_JOURNAL_LEN as u64,
    };

    const fn file(self) -> u64 {
        self.file
    }

    const fn journal(self) -> u64 {
        self.journal
    }
}

#[cfg(not(feature = "internals"))]
impl Bounds {
    const MAX: Self = Self;

    const fn file(self) -> u64 {
        SessionLog::MAX_FILE_LEN as u64
    }

    const fn journal(self) -> u64 {
        SessionLog::MAX_JOURNAL_LEN as u64
    }
}

impl SessionLog {
    /// The most bytes a log's file holds, which the log refuses an entry past. Its positions,
    /// input indices, payload offsets and the count of entries each tick ends at, are `u32`, and
    /// each input, payload byte and entry takes a byte of the file at least, so every position of
    /// a file within the bound fits. A match of 30 ticks a second that fills it runs for weeks.
    pub const MAX_FILE_LEN: usize = u32::MAX as usize;

    /// The most bytes a log's journal holds, which a restore streams through a buffer of one
    /// record, and which the log refuses an entry past. The journal frames each record and seal,
    /// and keeps what a load dropped, so it grows faster than the file.
    pub const MAX_JOURNAL_LEN: usize = u32::MAX as usize;

    /// A log with nothing recorded; an error when the header starts another count of player slots
    /// than the terms plan, or a player slot otherwise than its plan, when a player's delegation
    /// names another server or session than `header`, whose terms the session id hashes, or the
    /// main key of an earlier player slot's player, when there are more player slots than a `u32`
    /// counts, or when the terms let a journal record hold more than a journal frame takes, so no
    /// entry the log takes later can.
    pub fn new(header: SessionHeader) -> Result<Self, HeaderError> {
        Self::within(header, Bounds::MAX)
    }

    /// A log with nothing recorded, as `new` makes one, whose file and journal hold at most
    /// `bounds`.
    fn within(header: SessionHeader, bounds: Bounds) -> Result<Self, HeaderError> {
        if u32::try_from(header.player_slots.len()).is_err() {
            return Err(HeaderError::TooManyPlayerSlots);
        }
        if header.player_slots.len() != header.terms.player_slots.len() {
            return Err(HeaderError::PlayerSlotCount);
        }
        let largest = header
            .terms
            .largest_record()
            .filter(|&largest| usize::try_from(largest).is_ok_and(|largest| largest <= MAX_RECORD))
            .ok_or(HeaderError::RecordTooLarge)?;
        let session_id = header.terms.session_id();
        let player_slots_len = header.player_slots.len();
        let mut player_slots = Vec::with_capacity(header.player_slots.len());
        let mut entries = LogEntries::default();
        let mut main_keys = Vec::new();
        for ((player_slot, start), &plan) in (0..)
            .zip(&header.player_slots)
            .zip(&header.terms.player_slots)
        {
            let player_slot = PlayerSlot::new(player_slot);
            if start.plan() != plan {
                return Err(HeaderError::PlanMismatch { player_slot });
            }
            let control = match start {
                PlayerSlotStart::Player(delegation) => {
                    delegation
                        .check(&header.terms.server_key, &session_id)
                        .map_err(|error| HeaderError::Scope { player_slot, error })?;
                    if main_keys.contains(delegation.main_key()) {
                        return Err(HeaderError::MainKeyRepeated { player_slot });
                    }
                    main_keys.push(*delegation.main_key());
                    let index = entries.push_delegation((**delegation).clone());
                    Control::Player {
                        delegation: index,
                        chain: InputChain::new(player_slot, delegation.chain_root()),
                    }
                }
                PlayerSlotStart::Bot => Control::Bot,
                PlayerSlotStart::Open => Control::Open,
            };
            player_slots.push(PlayerSlotState::of(control));
        }
        let mut log = Self {
            entries,
            control: player_slots.into_iter().collect(),
            schedule: InputSchedule::new(player_slots_len, header.terms.inputs),
            segments: Segments::default(),
            loaded: false,
            base: None,
            result: None,
            header,
            revealed: None,
            checks: SignatureChecks::new(),
            message: Vec::new(),
            record: Vec::new(),
            server_since: 0,
            changes: Vec::new(),
            sealed_changes: 0..0,
            bounds,
            file_len: 0,
            unit: largest + RECORD_SLACK,
            session_id,
        };
        // The tag and the header, then a byte each: the first segment's number, 0, one segment,
        // of no tick, no entry since, and neither the seed nor the result.
        let mut start = file::LOG_TAG.to_vec();
        log.put_header(&mut start);
        log.file_len = len(start.len()) + 6;
        Ok(log)
    }

    /// Each input the log applied, in the order of its ticks, from its latest checkpoint on, as a
    /// replay of it seals them.
    pub fn applied_inputs(&self) -> Vec<AppliedInput> {
        let mut inputs = Vec::new();
        let Some(mut replay) = LogReplay::of(self) else {
            return inputs;
        };
        while !replay.is_done(self) {
            let tick = replay.next_tick();
            inputs.extend(replay.seal_tick(self).map(|input| AppliedInput {
                player_slot: input.player_slot,
                stamp: input.stamp,
                tick,
            }));
        }
        inputs
    }

    /// Whether the log may refuse its next entry: its file or `journal`, when its server keeps
    /// one, has no room for one more record of the most bytes and the session's end after it. A
    /// log that refused an entry is full. Its server ends the session at the boundary where it
    /// finds the log full, which the room the log keeps lets it do.
    pub fn is_full(&self, journal: Option<&Journal>) -> bool {
        let growth = Growth {
            file: self.unit,
            journal: self.unit,
        };
        !self.has_room(growth, journal)
    }

    /// Whether the file and `journal` take `growth` and keep the room the session's end takes
    /// after it, as `kept` counts it. A decode of the file, which records a checkpoint's record as
    /// its segment starts, then takes every entry the log took.
    fn has_room(&self, growth: Growth, journal: Option<&Journal>) -> bool {
        let kept = self.kept();
        let fits = |len: u64, more: u64, bound: u64| {
            len.checked_add(more)
                .and_then(|len| len.checked_add(kept))
                .is_some_and(|len| len <= bound)
        };
        fits(self.file_len, growth.file, self.bounds.file())
            && journal.is_none_or(|journal| {
                fits(journal.file_len(), growth.journal, self.bounds.journal())
            })
    }

    /// The bytes the session's end may still take: `END_RECORDS` records, and the record of each
    /// checkpoint begun that has none yet.
    fn kept(&self) -> u64 {
        self.unit * (END_RECORDS + len(self.segments.begun_count()))
    }

    /// What the entry `record` holds adds: to the file, its kind's byte, its body, and the
    /// growth of the count of the entries since the last tick; to the journal, its frame and its
    /// record.
    fn entry_growth(&self) -> Growth {
        let since = u64::from(self.entries.since_sealed());
        let record = len(self.record.len());
        Growth {
            file: record + varint_len(since + 1) - varint_len(since),
            journal: len(journal_frames::FRAME_OVERHEAD) + record,
        }
    }

    /// Notes the entry the log took last, of `growth`, in the file's length, and frames its
    /// record into `journal`, when its server keeps one.
    fn took_entry(&mut self, growth: Growth, journal: Option<&mut Journal>) {
        self.file_len += growth.file;
        debug_assert!(
            self.file_len <= self.bounds.file(),
            "the file stays within its bound"
        );
        if let Some(journal) = journal {
            let before = journal.file_len();
            journal.append(&self.record);
            debug_assert_eq!(
                journal.file_len(),
                before + growth.journal,
                "an entry's record adds what the log counted"
            );
        }
    }

    pub const fn header(&self) -> &SessionHeader {
        &self.header
    }

    /// The hash of the header's terms.
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Adds the last segment's server seed, which reveals every earlier one and publishes the
    /// log.
    pub fn reveal_seed(&mut self, server_seed: ServerSeed) {
        assert!(
            server_seed.check(self.segment(), &self.header.terms.seed_commitment),
            "the server reveals the seed it committed to"
        );
        self.set_revealed(server_seed);
    }

    /// Keeps `server_seed` as the revealed seed, which the file then holds in place of none.
    fn set_revealed(&mut self, server_seed: ServerSeed) {
        debug_assert!(self.revealed.is_none(), "the seed is revealed once");
        self.revealed = Some(server_seed);
        self.message.clear();
        put(&mut self.message, &self.revealed);
        self.file_len += len(self.message.len()) - 1;
        debug_assert!(self.file_len <= self.bounds.file(), "the end has its room");
    }

    /// Every segment's server seed, once the log is published.
    pub fn revealed_seeds(&self) -> Option<ServerSeeds> {
        (self.revealed.as_ref()).map(|seed| ServerSeeds::new(self.segment(), seed.clone()))
    }

    /// The last segment, which the next tick runs in.
    pub fn segment(&self) -> u32 {
        self.segments.last()
    }

    /// The checkpoint record of the segment that starts at `tick`, when one does and its record
    /// came.
    pub fn checkpoint_at(&self, tick: Tick) -> Option<&Checkpoint> {
        let segment = self.segments.get(self.segment_starting(tick)?)?;
        segment.checkpoint.as_ref().map(|logged| &logged.record)
    }

    /// The segment that starts at `tick`, its record come or not; none at tick 0, where the
    /// first starts from no checkpoint.
    pub fn segment_starting(&self, tick: Tick) -> Option<u32> {
        self.segments.starting(tick)
    }

    /// Whether the log went back to a save: a load it took, or one in the journal it was rebuilt
    /// from. A player's chain may then stand before what their client holds.
    pub const fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// The checkpoint the log began last and has no record of yet: the one at the boundary
    /// before the next tick, when the log began one there.
    pub fn begun_checkpoint(&self) -> Option<&CheckpointBegun> {
        self.segments.latest_begun()
    }

    /// The checkpoint its file starts at, whose snapshot its replay starts from; none for a log
    /// from tick 0.
    pub fn starts_at(&self) -> Option<&Checkpoint> {
        let first = self.segments.first();
        let start = self.segments.get(first).filter(|_| first > 0)?;
        start.checkpoint.as_ref().map(|logged| &logged.record)
    }

    /// The first segment whose entries the log holds, and the tick it starts at: the first a log
    /// file holds, the base a server's log forgot the past before, or segment 0 at tick 0.
    pub fn first_held(&self) -> HeldStart {
        let segment = self.base.unwrap_or_else(|| self.segments.first());
        let tick = self
            .segments
            .get(segment)
            .map_or(Tick::ZERO, |start| start.tick);
        HeldStart { segment, tick }
    }

    /// The entries of `tick`, in the order logged; none for a tick the log does not hold: one not
    /// sealed yet, or before the segment its entries start at, the first a log file holds or the
    /// base a server's log forgot the past before.
    pub fn tick_entries(&self, tick: Tick) -> Option<impl Iterator<Item = LogEntry<'_>>> {
        if tick < self.first_held().tick || tick >= self.next_tick() {
            return None;
        }
        let entries = &self.entries;
        let range = entries.tick_range(tick);
        Some(
            entries
                .entries(range)
                .map(move |entry| LogEntry::of(entries, entry)),
        )
    }

    /// The entries logged since the last tick was sealed, in the order logged, which the next
    /// tick takes.
    pub fn open_entries(&self) -> impl Iterator<Item = LogEntry<'_>> {
        let entries = &self.entries;
        entries
            .entries(entries.sealed_end()..entries.len())
            .map(move |entry| LogEntry::of(entries, entry))
    }

    /// Every checkpoint record it holds, by segment.
    pub fn checkpoints(&self) -> impl Iterator<Item = &Checkpoint> {
        self.segments.checkpoints().map(|logged| &logged.record)
    }

    /// How the session ended, once it did.
    pub fn result(&self) -> Option<&SessionResult> {
        self.result.as_ref().map(|logged| &logged.result)
    }

    /// The tick that the entries recorded now arrive before.
    pub fn next_tick(&self) -> Tick {
        self.entries.next_tick()
    }

    /// Where the next server input stands: before the next tick, after the server inputs logged
    /// there so far.
    pub fn next_place(&self) -> InputPlace {
        InputPlace {
            tick: self.next_tick(),
            index: self.server_since,
        }
    }

    /// The last stamp of the inputs `player_slot`'s player sent since it joined, below which the
    /// log refuses the next; none before its first, or for a player slot the session does not have.
    pub fn last_stamp(&self, player_slot: PlayerSlot) -> Option<Tick> {
        self.control.last_stamp(player_slot)
    }

    /// Who controls `player_slot` now; none for a player slot the session does not have.
    pub fn controller(&self, player_slot: PlayerSlot) -> Option<Controller<'_>> {
        self.control.controller(player_slot, &self.entries)
    }

    /// The main key of the player who left `player_slot` last, while no player took it since.
    pub fn leaver(&self, player_slot: PlayerSlot) -> Option<MainKey> {
        self.control.leaver(player_slot)
    }

    /// How many player slots the session has.
    pub fn player_slot_count(&self) -> u32 {
        offset(self.control.len())
    }

    /// The player slots a player controls now, in order.
    pub fn played_player_slots(&self) -> impl Iterator<Item = PlayerSlot> {
        self.control.played_player_slots()
    }

    /// Every change of a player slot's controller logged so far, in the order logged.
    pub fn changes(&self) -> &[PlayerSlotChange] {
        &self.changes
    }

    /// The changes of a player slot's controller that apply in the tick last sealed.
    pub fn sealed_changes(&self) -> &[PlayerSlotChange] {
        &self.changes[self.sealed_changes.clone()]
    }

    /// Logs a player's packet before the next tick. Writes when each input applies into
    /// `applied`, which is cleared first: early when its stamp is more than the max input lead
    /// ahead; else at `max(stamp, next tick)`, or later,
    /// when the player's inputs before it fill that tick: a player's inputs fill each tick up to
    /// the max inputs per tick, in chain order, and the rest spill to the next. A packet is
    /// refused for what its client controls alone, so one that follows the rules is never
    /// refused, however the network groups its packets: more inputs than the max inputs per
    /// tick, in the packet or of one stamp; a stamp before the one before it; more inputs in all
    /// than the ticks up to the max input lead hold; a payload past the max length; or a bad
    /// signature; and for a player slot its player does not control now, or when the log's file or
    /// journal has no room for it and the session's end. Each count and length is checked before
    /// any input is hashed. A refused packet leaves the log unchanged.
    pub fn record<'a, I>(
        &mut self,
        packet: PlayerPacket<'_, I>,
        applied: &mut Vec<Applied>,
        journal: Option<&mut Journal>,
    ) -> Result<(), InputError>
    where
        I: IntoIterator<Item = (Tick, &'a [u8])>,
        I::IntoIter: Clone,
    {
        let PlayerPacket {
            player_slot,
            inputs,
            signature,
        } = packet;
        let inputs = inputs.into_iter();
        if inputs.clone().next().is_none() {
            return Err(InputError::EmptyPacket);
        }
        debug_assert!(self.result.is_none(), "a session that ended takes no input");
        let terms = &self.header.terms;
        let state = *self
            .control
            .get(player_slot)
            .ok_or(InputError::UnknownPlayer)?;
        let Control::Player {
            delegation,
            mut chain,
        } = state.control
        else {
            return Err(InputError::NotPlayer);
        };
        let session_key = self.entries.delegation(delegation).terms().session_key;
        let mut stamps = state.stamps;
        let mut count = 0;
        for (stamp, payload) in inputs.clone() {
            if len(payload.len()) > u64::from(terms.inputs.max_payload_len) {
                return Err(InputError::PayloadTooLarge);
            }
            count += 1;
            if count > terms.inputs.max_per_tick {
                return Err(InputError::TooManyInputs);
            }
            stamps.add(stamp, terms.inputs.max_per_tick)?;
        }
        let ticks = self
            .next_tick()
            .get()
            .saturating_add(terms.inputs.max_lead.get())
            .saturating_add(1);
        if stamps.total > ticks.saturating_mul(u64::from(terms.inputs.max_per_tick))
            || !self.schedule.admits(
                player_slot,
                inputs.clone().map(|(stamp, _)| stamp),
                self.next_tick(),
            )
        {
            return Err(InputError::AheadOfTime);
        }
        self.record.clear();
        self.record.push(journal_kinds::JOURNAL_PACKET);
        let written = PlayerPacket {
            player_slot,
            inputs: inputs.clone(),
            signature,
        };
        file::write_packet(&mut self.record, written);
        let growth = self.entry_growth();
        if !self.has_room(growth, journal.as_deref()) {
            return Err(InputError::LogFull);
        }
        for (stamp, payload) in inputs.clone() {
            chain.extend(stamp, payload);
        }
        let head = chain.signed_head(self.session_id);
        if !self.checks.verifies(signature, &session_key, &head) {
            return Err(InputError::BadSignature);
        }

        let taken = TakenPacket {
            delegation,
            chain,
            stamps,
        };
        self.control.took_packet(player_slot, taken);
        applied.clear();
        applied.reserve_exact(at(count));
        let next = self.next_tick();
        let start = self.entries.inputs_len();
        for (stamp, payload) in inputs {
            let index = self.entries.push_input(PlayerInput {
                player_slot,
                stamp,
                payload,
            });
            applied.push(self.schedule.schedule(&self.entries, index, next));
        }
        let end = self.entries.inputs_len();
        self.entries.push_packet(start..end, *signature);
        let mut journal = journal;
        self.took_entry(growth, journal.as_deref_mut());
        if let Some(journal) = journal {
            let head = DurableHead {
                delegation: *self.entries.delegation(delegation).id(),
                seq: chain.next_seq() - 1,
                head: chain.head(),
            };
            let record = journal.written() - 1;
            journal.durable_mut().journaled(record, player_slot, head);
        }
        Ok(())
    }

    /// What a server input would do to its player slot's controller, logged now, as its structure
    /// allows, before its signature is checked: a join from what the player slot is, a leave to
    /// what it becomes, or nothing; an error for an input the log refuses. A bot's commands need a
    /// player slot a bot plays, a payload within the max length and a place, as a player's inputs
    /// spill past a tick's max inputs, within the max input lead past the next tick; a join a
    /// player slot no player controls, and of a reserved one its leaver, by a main key that
    /// controls no other player slot; a renewal, a leave and a link's change a player slot a player
    /// controls, a renewal by the same main key; a delegation this session's.
    fn change_of(
        &self,
        input: &ServerInput<'_>,
    ) -> Result<Option<PlayerSlotChangeKind>, ServerInputError> {
        let terms = &self.header.terms;
        let state = self
            .control
            .get(input.player_slot())
            .ok_or(ServerInputError::UnknownPlayerSlot)?;
        let scope = |delegation: &Delegation| {
            delegation
                .check(&terms.server_key, &self.session_id)
                .map_err(ServerInputError::Scope)
        };
        let player_key = match state.control {
            Control::Player { delegation, .. } => {
                Some(*self.entries.delegation(delegation).main_key())
            }
            Control::Bot | Control::Open | Control::Reserved => None,
        };
        match input {
            ServerInput::Bot { payload, .. } => {
                if !matches!(state.control, Control::Bot) {
                    return Err(ServerInputError::NotBot);
                }
                if len(payload.len()) > u64::from(terms.inputs.max_payload_len) {
                    return Err(ServerInputError::PayloadTooLarge);
                }
                let next = self.next_tick();
                if !self.schedule.admits(input.player_slot(), [next], next) {
                    return Err(ServerInputError::AheadOfTime);
                }
                Ok(None)
            }
            ServerInput::Join { delegation, .. } => {
                scope(delegation)?;
                let own = state.leaver == Some(*delegation.main_key());
                let from = match state.control {
                    Control::Player { .. } => return Err(ServerInputError::Occupied),
                    _ if own => Taken::Own,
                    Control::Reserved => return Err(ServerInputError::Reserved),
                    Control::Open => Taken::Open,
                    Control::Bot => Taken::Bot,
                };
                if self.control.holds(delegation.main_key(), &self.entries) {
                    return Err(ServerInputError::MainKeyHeld);
                }
                Ok(Some(PlayerSlotChangeKind::Joined { from }))
            }
            ServerInput::Renew { delegation, .. } => {
                let main_key = player_key.ok_or(ServerInputError::NotPlayer)?;
                if main_key != *delegation.main_key() {
                    return Err(ServerInputError::OtherPlayer);
                }
                scope(delegation)?;
                Ok(None)
            }
            ServerInput::Leave { becomes, .. } => {
                player_key.ok_or(ServerInputError::NotPlayer)?;
                Ok(Some(PlayerSlotChangeKind::Left { becomes: *becomes }))
            }
            ServerInput::Connected { .. } | ServerInput::Disconnected { .. } => {
                player_key.ok_or(ServerInputError::NotPlayer)?;
                Ok(None)
            }
        }
    }

    /// Logs a server input before the next tick, with the server key's `signature` over it at its
    /// place, the next server input logged there: what `change_of` allows, and the log's file and
    /// journal have room for, a bot's commands applying in the next tick in player slot order,
    /// among the players' inputs, or, past the tick's max inputs, in the first later tick its
    /// player slot's inputs leave room in. A join or a leave of a player slot drops the inputs its
    /// player logged that were still to apply, and changes the controller from the next tick on. A
    /// refused input leaves the log unchanged.
    pub(crate) fn record_server(
        &mut self,
        input: ServerInput<'_>,
        signature: &Signature,
        journal: Option<&mut Journal>,
    ) -> Result<(), ServerInputError> {
        let change = self.server_change(&input)?;
        let placed = EncodedPlace {
            session_id: self.session_id,
            place: self.next_place(),
            input: &self.record[1..],
        };
        let server_key = self.header.terms.server_key;
        if !self.checks.verifies(signature, &server_key, &placed) {
            return Err(ServerInputError::BadSignature);
        }
        self.log_server(input, change, signature, journal)
    }

    /// Logs a server input as `record_server` does, signed here by `server`, of the terms'
    /// server key. A signature the log made over the message it wrote needs no check, which is
    /// the most a server input costs.
    pub fn serve(
        &mut self,
        input: ServerInput<'_>,
        server: &Signer,
        journal: Option<&mut Journal>,
    ) -> Result<(), ServerInputError> {
        self.serving(input)?.log(server, journal)
    }

    /// `input` checked as `change_of` checks it, its record written, to be logged by
    /// `ServerServe::log` once what it does to its player slot's controller passes a caller's rule.
    pub fn serving<'l, 'i>(
        &'l mut self,
        input: ServerInput<'i>,
    ) -> Result<ServerServe<'l, 'i>, ServerInputError> {
        let change = self.server_change(&input)?;
        Ok(ServerServe {
            log: self,
            input,
            change,
        })
    }

    /// Checks that `server` holds the terms' server key, which signs what the server logs.
    fn debug_server(&self, server: &Signer) {
        debug_assert_eq!(
            server.public_key(),
            self.header.terms.server_key,
            "the server signs with the terms' server key"
        );
    }

    /// What `input` does to its player slot's controller, when `change_of` allows it, its record,
    /// its kind's byte and its encoding, written into the log's record.
    fn server_change(
        &mut self,
        input: &ServerInput<'_>,
    ) -> Result<Option<PlayerSlotChangeKind>, ServerInputError> {
        let change = self.change_of(input)?;
        self.record.clear();
        self.record.push(journal_kinds::JOURNAL_SERVER);
        input.encode(&mut self.record);
        Ok(change)
    }

    /// Logs a server input whose record `server_change` wrote, with its `signature`, when the
    /// file and `journal` have room for it.
    fn log_server(
        &mut self,
        input: ServerInput<'_>,
        change: Option<PlayerSlotChangeKind>,
        signature: &Signature,
        journal: Option<&mut Journal>,
    ) -> Result<(), ServerInputError> {
        put(&mut self.record, signature);
        let growth = self.entry_growth();
        if !self.has_room(growth, journal.as_deref()) {
            return Err(ServerInputError::LogFull);
        }
        debug_assert!(self.result.is_none(), "a session that ended takes no input");
        let player_slot = input.player_slot();
        let next = self.next_tick();
        let served = match input {
            ServerInput::Bot { payload, .. } => {
                let index = self.entries.push_input(PlayerInput {
                    player_slot,
                    stamp: next,
                    payload,
                });
                self.schedule.schedule(&self.entries, index, next);
                Served::Bot { input: index }
            }
            ServerInput::Join { delegation, .. } => {
                let chain = InputChain::joined(player_slot, delegation.id(), self.next_place());
                let index = self.entries.push_delegation(delegation);
                self.control.join(player_slot, index, chain);
                self.schedule.forget(player_slot);
                Served::Join {
                    player_slot,
                    delegation: index,
                }
            }
            // The chain's head stays the old key's until the new key signs an input: a receipt
            // names the key that signed its head, which the player's client then no longer holds.
            ServerInput::Renew { delegation, .. } => {
                let index = self.entries.push_delegation(delegation);
                self.control.renew(player_slot, index);
                Served::Renew {
                    player_slot,
                    delegation: index,
                }
            }
            ServerInput::Leave {
                reason, becomes, ..
            } => {
                self.control.leave(player_slot, becomes, &self.entries);
                self.schedule.forget(player_slot);
                Served::Leave {
                    player_slot,
                    reason,
                    becomes,
                }
            }
            ServerInput::Connected { .. } => Served::Connected { player_slot },
            ServerInput::Disconnected { .. } => Served::Disconnected { player_slot },
        };
        if let Some(kind) = change {
            self.changes.push(PlayerSlotChange {
                tick: next,
                player_slot,
                kind,
            });
        }
        self.entries.push_server(served, *signature);
        self.server_since += 1;
        let mut journal = journal;
        self.took_entry(growth, journal.as_deref_mut());
        if let Some(journal) = journal {
            match served {
                Served::Join { .. } | Served::Renew { .. } | Served::Leave { .. } => {
                    journal.durable_mut().forget(player_slot);
                }
                Served::Bot { .. } | Served::Connected { .. } | Served::Disconnected { .. } => {}
            }
        }
        Ok(())
    }

    /// Ends the last segment at the boundary before the next tick, and starts the next there, as a
    /// checkpoint begins: from then on the sim draws from the new segment's seed. Its record, which
    /// the server signs once its snapshot is written, comes later by `record_checkpoint`, after the
    /// records of the checkpoints begun before it. The last segment must hold a tick. The segment,
    /// the tick, and the log's own state there, which the record must carry. A checkpoint comes at
    /// a tick boundary, before any entry of the tick after it.
    pub fn begin_checkpoint(
        &mut self,
        journal: Option<&mut Journal>,
    ) -> Result<CheckpointBegun, CheckpointError> {
        assert!(
            self.result.is_none(),
            "a session that ended takes no checkpoint"
        );
        assert!(
            self.is_at_boundary(),
            "a checkpoint comes before any entry of its first tick"
        );
        let tick = self.next_tick();
        if self.segments.last_start().tick == tick {
            return Err(CheckpointError::Empty);
        }
        let number = self.segments.next();
        // The count of segments grows, and the new one's count of ticks, 0, takes a byte.
        self.file_len += varint_len(u64::from(number) + 1) - varint_len(u64::from(number)) + 1;
        let begun = CheckpointBegun {
            segment: number,
            tick,
            carry: self.carry(),
        };
        let boundary = Boundary {
            entries: self.entries.at(),
            head_signers: self
                .control
                .iter()
                .map(|player_slot| player_slot.head_signer)
                .collect(),
            pending: self.schedule.pending().into_iter().copied().collect(),
            file_len: self.file_len,
        };
        self.segments.begin(begun.clone(), boundary);
        self.record.clear();
        journal_records::write_begun(&mut self.record, number, tick);
        self.journal_record(journal);
        Ok(begun)
    }

    /// Logs `record` as `record_checkpoint` does, the past before its boundary held where `past`
    /// says.
    pub(super) fn record_checkpoint_in(
        &mut self,
        record: Checkpoint,
        signature: &Signature,
        journal: Option<&mut Journal>,
        past: Past,
    ) -> Result<(), CheckpointError> {
        self.segments
            .oldest_begun()
            .ok_or(CheckpointError::NotBegun)?;
        let server_key = &self.header.terms.server_key;
        let id = self.session_id;
        let signed = InSession {
            session_id: id,
            record: &record,
        };
        let server_key = *server_key;
        if !self.checks.verifies(signature, &server_key, &signed) {
            return Err(CheckpointError::BadSignature);
        }
        self.take_checkpoint(record, signature, journal, past)
    }

    /// Logs `record` as `record_checkpoint` does, signed here by `server`, of the terms' server
    /// key, whose signature over the record the log made needs no check.
    pub fn serve_checkpoint(
        &mut self,
        record: Checkpoint,
        server: &Signer,
        journal: Option<&mut Journal>,
    ) -> Result<(), CheckpointError> {
        self.debug_server(server);
        let signature = server.sign_with(
            &InSession {
                session_id: self.session_id,
                record: &record,
            },
            &mut self.message,
        );
        let past = if journal.is_some() {
            Past::InJournal
        } else {
            Past::InLog
        };
        self.take_checkpoint(record, &signature, journal, past)
    }

    /// Logs `record`, whose `signature` is the server's, as the record of the oldest checkpoint
    /// begun, the past before its boundary held where `past` says.
    fn take_checkpoint(
        &mut self,
        record: Checkpoint,
        signature: &Signature,
        journal: Option<&mut Journal>,
        past: Past,
    ) -> Result<(), CheckpointError> {
        let begun = self
            .segments
            .oldest_begun()
            .ok_or(CheckpointError::NotBegun)?;
        if record.segment != begun.segment {
            return Err(CheckpointError::Segment);
        }
        if record.tick != begun.tick {
            return Err(CheckpointError::Tick);
        }
        if record.carry != begun.carry {
            return Err(CheckpointError::Carry);
        }
        let logged = LoggedCheckpoint {
            record,
            signature: *signature,
        };
        self.record.clear();
        logged.write(&mut self.record);
        let bytes = len(self.record.len()) - 1;
        self.file_len += bytes;
        debug_assert!(
            self.file_len <= self.bounds.file(),
            "a checkpoint has its room"
        );
        self.journal_record(journal);
        let segment = self.segments.record(logged, bytes);
        if past == Past::InJournal {
            self.forget_before(segment);
        }
        Ok(())
    }

    /// The log's own state at the boundary before the next tick, as a checkpoint there carries
    /// it.
    fn carry(&self) -> LogCarry {
        let player_slots = (0..)
            .map(PlayerSlot::new)
            .zip(self.control.iter())
            .map(|(at, player_slot)| CarriedPlayerSlot {
                control: player_slot
                    .control
                    .map(|delegation| Box::new(self.entries.delegation(delegation).clone())),
                leaver: player_slot.leaver,
                stamps: player_slot.stamps,
                spill: self.schedule.spill(at),
            })
            .collect();
        let pending = self
            .schedule
            .pending()
            .into_iter()
            .map(|due| {
                let input = self.entries.input(due.index);
                CarriedInput {
                    tick: due.tick,
                    player_slot: due.player_slot,
                    stamp: input.stamp,
                    payload: input.payload.to_vec(),
                }
            })
            .collect();
        LogCarry {
            player_slots,
            pending,
        }
    }

    /// Ends the session with `result`, with the server key's `signature` over it, which must
    /// stop before the next tick. A refused result leaves the log unchanged.
    pub(crate) fn record_result(
        &mut self,
        result: SessionResult,
        signature: &Signature,
        journal: Option<&mut Journal>,
    ) -> Result<(), ResultError> {
        let server_key = &self.header.terms.server_key;
        let id = self.session_id;
        let signed = InSession {
            session_id: id,
            record: &result,
        };
        let server_key = *server_key;
        if !self.checks.verifies(signature, &server_key, &signed) {
            return Err(ResultError::BadSignature);
        }
        self.take_result(result, signature, journal)
    }

    /// Ends the session with `result` as `record_result` does, signed here by `server`, of the
    /// terms' server key, whose signature over the result the log made needs no check.
    pub fn serve_result(
        &mut self,
        result: SessionResult,
        server: &Signer,
        journal: Option<&mut Journal>,
    ) -> Result<(), ResultError> {
        self.debug_server(server);
        let signature = server.sign_with(
            &InSession {
                session_id: self.session_id,
                record: &result,
            },
            &mut self.message,
        );
        self.take_result(result, &signature, journal)
    }

    /// Ends the session with `result`, whose `signature` is the server's.
    fn take_result(
        &mut self,
        result: SessionResult,
        signature: &Signature,
        journal: Option<&mut Journal>,
    ) -> Result<(), ResultError> {
        assert!(self.result.is_none(), "a session ends once");
        if result.tick != self.next_tick() {
            return Err(ResultError::Tick);
        }
        let logged = LoggedResult {
            result,
            signature: *signature,
        };
        self.record.clear();
        journal_records::write_result(&mut self.record, &logged);
        // The file's option of the result takes the record's body in place of its none's byte.
        self.file_len += len(self.record.len()) - 1;
        debug_assert!(self.file_len <= self.bounds.file(), "the end has its room");
        self.journal_record(journal);
        self.result = Some(logged);
        Ok(())
    }

    /// Closes the next tick to new entries and gives the inputs applied in it, by player slot, then
    /// in the order logged; `sealed_changes` then gives the changes of a player slot's controller
    /// in it.
    pub fn seal_tick(
        &mut self,
        journal: Option<&mut Journal>,
    ) -> impl ExactSizeIterator<Item = PlayerInput<'_>> {
        self.seal(journal);
        self.schedule
            .due()
            .iter()
            .map(|&index| self.entries.input(index))
    }

    /// Closes the next tick to new entries, and has the schedule give the inputs applied in it.
    fn seal(&mut self, journal: Option<&mut Journal>) {
        let tick = self.next_tick();
        // The segment's count of ticks grows, and the next tick's count of entries, 0, takes a
        // byte.
        let ticks = tick.get() - self.segments.last_start().tick.get();
        self.file_len += varint_len(ticks + 1) - varint_len(ticks) + 1;
        debug_assert!(self.file_len <= self.bounds.file(), "a seal has its room");
        self.record.clear();
        self.record.push(journal_kinds::JOURNAL_SEALED);
        self.journal_record(journal);
        self.entries.seal(self.entries.len());
        self.server_since = 0;
        let first = self.changes.partition_point(|change| change.tick < tick);
        let after = self.changes.partition_point(|change| change.tick <= tick);
        self.sealed_changes = first..after;
        self.schedule.seal(tick);
    }

    /// Goes back to the boundary where segment `segment` starts, its checkpoint's, as a load of
    /// that save does: the log becomes what it held there, from the checkpoint's carry and the
    /// log's own state at the boundary, its segments after that one and every entry logged after
    /// the boundary dropped, and the segment goes on from its checkpoint, with its seed
    /// (08-sessions.md, What reads the past reads the carry). `journal` logs the load, so a log
    /// rebuilt from it is the log loaded, and forgets where each chain stood durably. An error
    /// when the log holds no checkpoint that starts the segment, as one before the base of a log
    /// whose journal holds what came before, and when `journal` has no room for the load's
    /// record.
    pub fn load(
        &mut self,
        segment: u32,
        journal: Option<&mut Journal>,
    ) -> Result<(), LogLoadError> {
        let start = self.segments.get(segment);
        let saved =
            start.and_then(|start| Some((start.checkpoint.as_ref()?, start.boundary.as_ref()?)));
        let Some((logged, boundary)) = saved else {
            return Err(LogLoadError::NoCheckpoint);
        };
        let record = Growth {
            file: 0,
            journal: len(journal_frames::FRAME_OVERHEAD) + 1 + varint_len(u64::from(segment)),
        };
        if !self.has_room(record, journal.as_deref()) {
            return Err(LogLoadError::LogFull);
        }
        debug_assert!(
            self.revealed.is_none() && self.result.is_none(),
            "a session that ended loads nothing"
        );
        let tick = logged.record.tick;
        let carry = self
            .check_carry(&logged.record)
            .expect("a log's own checkpoint carries its state");
        let boundary = boundary.clone();
        self.entries.truncate(boundary.entries);
        self.segments.truncate_after(segment);
        self.take_carry(carry, boundary.head_signers);
        self.changes.retain(|change| change.tick < tick);
        let sealed = tick.get().checked_sub(1).map(Tick::new);
        let first = self
            .changes
            .partition_point(|change| sealed.is_none_or(|sealed| change.tick < sealed));
        self.sealed_changes = first..self.changes.len();
        self.file_len = boundary.file_len;
        self.server_since = 0;
        self.loaded = true;
        if self.base.is_some() {
            self.base = Some(segment);
        }
        self.record.clear();
        journal_records::write_loaded(&mut self.record, segment);
        if let Some(journal) = journal {
            journal.append(&self.record);
            journal.durable_mut().clear();
        }
        Ok(())
    }

    /// Starts the log, a new one that took nothing, at the checkpoint `record`, which the server
    /// key's `signature` signs, as a log file that starts there gives it: the segment it starts
    /// is the log's first, the log holds no entry before its boundary, and its own state there is
    /// the record's carry, which the server signed. An error for a record the server key did not
    /// sign, one of segment 0 or at tick 0, and a carry no log holds, as `CheckedCarry` checks it.
    fn start_at_checkpoint(
        &mut self,
        record: Checkpoint,
        signature: &Signature,
    ) -> Result<(), CheckpointError> {
        debug_assert!(
            self.entries.len() == 0 && self.next_tick() == Tick::ZERO,
            "a log starts at a checkpoint before it takes anything"
        );
        let signed = InSession {
            session_id: self.session_id,
            record: &record,
        };
        let server_key = self.header.terms.server_key;
        if !self.checks.verifies(signature, &server_key, &signed) {
            return Err(CheckpointError::BadSignature);
        }
        if record.segment == 0 {
            return Err(CheckpointError::Segment);
        }
        if record.tick == Tick::ZERO {
            return Err(CheckpointError::Empty);
        }
        let carry = self
            .check_carry(&record)
            .map_err(CheckpointError::BadCarry)?;
        self.entries.start_at(record.tick);
        let entries = self.entries.at();
        self.take_carry(carry, iter::repeat(None));
        let logged = LoggedCheckpoint {
            record,
            signature: *signature,
        };
        self.record.clear();
        logged.write(&mut self.record);
        let segment = logged.record.segment;
        // The first segment's number, then its record, the byte of its kind left out.
        self.file_len +=
            varint_len(u64::from(segment)) - varint_len(0) + len(self.record.len()) - 1;
        let boundary = Boundary {
            entries,
            head_signers: vec![None; self.control.len()],
            pending: self.schedule.pending().into_iter().copied().collect(),
            file_len: self.file_len,
        };
        let start = SegmentStart {
            tick: logged.record.tick,
            checkpoint: Some(logged),
            boundary: Some(boundary),
        };
        self.segments = Segments::from_checkpoint(segment, start);
        self.base = Some(segment);
        Ok(())
    }

    /// The carry of `record`, once it holds a state this log holds at the record's boundary.
    fn check_carry(&self, record: &Checkpoint) -> Result<CheckedCarry, CarryFlaw> {
        CheckedCarry::new(
            record,
            CarryCx {
                terms: &self.header.terms,
                session_id: &self.session_id,
                player_slots: self.control.len(),
            },
        )
    }

    /// Takes `carry`, the log's own state at a checkpoint's boundary, as who controls each
    /// player_slot and when each input still to apply applies, each player slot's chain head signed
    /// by the delegation `head_signers` gives: what a load goes back to, and what a log file that
    /// starts at the checkpoint starts from. The carried inputs are logged again, after the entries
    /// the log holds.
    fn take_carry(
        &mut self,
        carry: CheckedCarry,
        head_signers: impl IntoIterator<Item = Option<u32>>,
    ) {
        let LogCarry {
            player_slots,
            pending,
        } = carry.into_inner();
        let mut control = Vec::with_capacity(player_slots.len());
        let mut spills = Vec::with_capacity(player_slots.len());
        for (carried, head_signer) in player_slots.into_iter().zip(head_signers) {
            let held = carried.control.map(|delegation| {
                self.entries
                    .delegation_index(&delegation)
                    .unwrap_or_else(|| self.entries.push_delegation(*delegation))
            });
            control.push(PlayerSlotState {
                control: held,
                leaver: carried.leaver,
                stamps: carried.stamps,
                head_signer,
            });
            spills.push(carried.spill);
        }
        let mut due = Vec::with_capacity(pending.len());
        for input in &pending {
            let index = self.entries.push_input(PlayerInput {
                player_slot: input.player_slot,
                stamp: input.stamp,
                payload: &input.payload,
            });
            due.push(InputOrder {
                tick: input.tick,
                player_slot: input.player_slot,
                index,
            });
        }
        self.control = control.into_iter().collect();
        self.schedule = InputSchedule::restored(self.header.terms.inputs, spills, due);
    }

    /// Forgets what the log took before the boundary where segment `segment` starts, which a
    /// server's journal holds: its entries, its ticks' ends, and its inputs but those still to
    /// apply (08-sessions.md, The live log is its latest checkpoint's state and the entries
    /// since).
    fn forget_before(&mut self, segment: u32) {
        let held = self
            .segments
            .get(segment)
            .and_then(|start| start.boundary.as_ref())
            .expect("a segment begun has its boundary");
        let boundary = held.entries;
        let pending = held.pending.iter().map(|due| due.index);
        let earliest = pending.chain(self.schedule.earliest_index()).min();
        let inputs = earliest.map_or(boundary.inputs, |earliest| earliest.min(boundary.inputs));
        self.entries.forget_before(boundary, inputs);
        self.segments.forget_before(segment);
        self.base = Some(segment);
    }

    /// Whether the log holds every entry it took: it forgot none, as a log with no journal.
    pub(super) const fn holds_all(&self) -> bool {
        self.base.is_none()
    }

    /// The segment from whose boundary the log holds its entries, once it forgot those before.
    pub(super) const fn base(&self) -> Option<u32> {
        self.base
    }

    /// Frames the record the log wrote last into `journal`, when its server keeps one.
    fn journal_record(&self, journal: Option<&mut Journal>) {
        if let Some(journal) = journal {
            journal.append(&self.record);
        }
    }
}

/// A position in the log's buffers, which the file's bound keeps within a `u32`.
fn offset(len: usize) -> u32 {
    u32::try_from(len).expect("log positions fit u32")
}

/// The index in the log's buffers of the position `position`, as `offset` gave it.
pub(crate) fn at(position: u32) -> usize {
    usize::try_from(position).expect("a u32 fits usize")
}

fn len(len: usize) -> u64 {
    u64::try_from(len).expect("a length fits u64")
}

/// The bytes postcard's varint of `value` takes: 7 bits a byte, at least one.
const fn varint_len(value: u64) -> u64 {
    let bits = value.bit_width();
    if bits == 0 {
        1
    } else {
        bits.div_ceil(7) as u64
    }
}

fn put<T: Serialize + ?Sized>(out: &mut Vec<u8>, value: &T) {
    Binary::encode_into(value, out);
}

/// Reads one value off the front of `rest`.
fn take<'a, T: Deserialize<'a> + Serialize>(rest: &mut &'a [u8]) -> Result<T, LogError> {
    let taken = Binary::take(rest).map_err(|error| match error {
        BinaryError::Truncated => LogError::Truncated,
        BinaryError::NotCanonical => LogError::NotCanonical,
        BinaryError::Malformed(malformed) => LogError::Malformed(malformed),
    })?;
    *rest = taken.rest;
    Ok(taken.value)
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::server_input::ServerInput;
    use crate::session_log::SessionLog;

    impl SessionLog {
        /// Every server input logged so far, in the order logged.
        pub fn server_inputs(&self) -> impl Iterator<Item = ServerInput<'_>> {
            self.entries
                .servers()
                .map(|logged| self.entries.server_input(logged.served))
        }

        /// Bounds the log's file to what it holds and the room the session's end takes after a
        /// record of the most bytes, less a byte: the log is full, as a session of weeks leaves
        /// it.
        pub fn fill(&mut self) {
            self.bounds.file = self.file_len + self.unit + self.kept() - 1;
            debug_assert!(self.is_full(None), "the log is full");
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod bench;

#[cfg(test)]
mod tests;
