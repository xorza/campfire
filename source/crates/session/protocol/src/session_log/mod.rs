use std::ops::Range;

use campfire_codec::{Binary, BinaryError};
use campfire_common::{PlayerSlot, Tick};
use secp256k1::XOnlyPublicKey;
use serde::{Deserialize, Serialize};

use crate::checkpoint::Checkpoint;
use crate::checkpoint::checkpoint_begun::CheckpointBegun;
use crate::checkpoint::log_carry::{CarriedControl, CarriedInput, CarriedSlot, LogCarry};
use crate::controller::Controller;
use crate::delegation::Delegation;
use crate::input_chain::InputChain;
use crate::journal::Journal;
use crate::journal::journal_frames::{self, MAX_RECORD};
use crate::main_key::MainKey;
use crate::player_input::PlayerInput;
use crate::server_input::{InputPlace, PlacedInput, ServerInput};
use crate::server_seed::ServerSeed;
use crate::server_seeds::ServerSeeds;
use crate::session_id::SessionId;
use crate::session_log::applied::Applied;
use crate::session_log::durable_head::DurableHead;
use crate::session_log::durable_heads::DurableHeads;
use crate::session_log::error::{
    CheckpointError, HeaderError, InputError, LogError, LogLoadError, ResultError, ServerInputError,
};
use crate::session_log::input_schedule::InputSchedule;
use crate::session_log::log_entries::{LogEntries, Served};
use crate::session_log::paged_vec::PagedVec;
use crate::session_log::player_packet::PlayerPacket;
use crate::session_log::segments::{LoggedCheckpoint, Segments};
use crate::session_log::session_header::SessionHeader;
use crate::session_log::slot_control::{Control, Slot, SlotControl, TakenPacket};
use crate::session_result::SessionResult;
use crate::signature::Signature;
use crate::signed::{InSession, Signed};
use crate::signer::Signer;
use crate::slot_change::{SlotChange, SlotChangeKind, Taken};
use crate::slot_start::SlotStart;
use crate::verifier::Verifier;

pub(crate) mod applied;
pub(crate) mod durable_head;
mod durable_heads;
pub(crate) mod error;
mod file;
mod input_schedule;
mod journal_records;
mod log_entries;
pub(crate) mod paged_bytes;
pub(crate) mod paged_vec;
pub(crate) mod player_packet;
mod segments;
pub(crate) mod session_header;
mod slot_control;

/// What one record adds beside its own bytes: in the journal its frame and its kind's byte, in
/// the file its kind's byte and the growth of the counts before it, each at most 10 bytes.
const RECORD_SLACK: u64 = 64;
/// The records the session's end may take past the room the last entry leaves, each of at most
/// a record's bytes: the begin of a checkpoint and its record, the seal of the tick before the
/// end, and the result with the revealed seed.
const END_RECORDS: u64 = 4;

/// The entries of a session, in memory, in the order they were logged, and grouped by the tick
/// that was next when each arrived: the players' packets and the server's inputs. The applied
/// tick follows from that group, so a log cannot hold a wrong one. A player's inputs arrive in
/// packets, each signed once over the player's chain head after it, and each server input is
/// signed over its place, so every logged entry is signed. The log follows who controls each
/// slot, a player, a bot, or none, from the server's inputs, and takes a player's packet only
/// for a slot the player controls. A checkpoint ends a segment and starts the next, whose seed
/// the sim draws from, and a result ends the session. The server records entries as they arrive;
/// decoding a published log records them again, which checks every chain link and signature, and
/// a verifier then replays the decoded log's ticks with `rewound`.
#[derive(Debug)]
pub struct SessionLog {
    header: SessionHeader,
    /// Present once the segment is published.
    revealed: Option<ServerSeed>,
    verifier: Verifier,
    /// Whether the log checks the signature of each record it takes, or trusts them, as it does
    /// while it decodes its own past again for a load.
    signatures: Signatures,
    /// The scratch a server input's message is written into as the log signs it.
    message: Vec<u8>,
    /// What the log holds, as it was logged.
    entries: LogEntries,
    /// Who controls each slot.
    control: SlotControl,
    /// When each input applies.
    schedule: InputSchedule,
    segments: Segments,
    /// Whether the log went back to a save: a load it took, or one in the journal it was
    /// rebuilt from.
    loaded: bool,
    /// How the session ended, once it did.
    result: Option<LoggedResult>,
    /// The server inputs logged since the last tick was sealed.
    server_since: u32,
    /// Every change of a slot's controller, in the order logged, so by tick.
    changes: Vec<SlotChange>,
    /// The changes of the tick last sealed.
    sealed_changes: Range<usize>,
    /// While a rewound log replays, the tick ends it had; empty otherwise.
    to_replay: PagedVec<u32>,
    /// The most bytes the log's file and its journal hold.
    bounds: Bounds,
    /// The bytes `encode` writes now.
    file_len: u64,
    /// The most bytes one record adds to the file or the journal: the terms' largest record and
    /// `RECORD_SLACK`.
    unit: u64,
    /// The hash of the header's terms.
    session_id: SessionId,
    /// Where each record the log takes goes as it takes it, once the server keeps one.
    journal: Option<Journal>,
    /// Where each slot's chain stands in the records the journal synced.
    durable: DurableHeads,
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

/// How the log takes a record's signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Signatures {
    /// It checks each against the key it must be of.
    Check,
    /// It trusts each, as the log's own past, whose every signature it checked as it first
    /// took it.
    Trust,
}

/// The bytes a record adds to the log's file and to its journal.
#[derive(Debug, Clone, Copy)]
struct Growth {
    file: u64,
    journal: u64,
}

/// What a server input that passes its checks does: the change of its slot's controller, and
/// what it adds to the file and the journal.
#[derive(Debug, Clone, Copy)]
struct ServerAllowed {
    change: Option<SlotChangeKind>,
    growth: Growth,
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

    /// The most bytes a log's journal holds, which a restore reads whole, and which the log
    /// refuses an entry past. The journal frames each record and seal, and keeps what a load
    /// dropped, so it grows faster than the file.
    pub const MAX_JOURNAL_LEN: usize = u32::MAX as usize;

    /// A log with nothing recorded; an error when the header starts another count of slots than
    /// the terms plan, or a slot otherwise than its plan, when a player's delegation names
    /// another server or session than `header`, whose terms the session id hashes, or the main
    /// key of an earlier slot's player, when there are more slots than a `u32` counts, or when
    /// the terms let a journal record hold more than a journal frame takes, so no entry the log
    /// takes later can.
    pub fn new(header: SessionHeader) -> Result<Self, HeaderError> {
        Self::within(header, Bounds::MAX)
    }

    /// A log with nothing recorded, as `new` makes one, whose file and journal hold at most
    /// `bounds`.
    fn within(header: SessionHeader, bounds: Bounds) -> Result<Self, HeaderError> {
        if u32::try_from(header.slots.len()).is_err() {
            return Err(HeaderError::TooManySlots);
        }
        if header.slots.len() != header.terms.slots.len() {
            return Err(HeaderError::SlotCount);
        }
        let largest = header
            .terms
            .largest_record()
            .filter(|&largest| usize::try_from(largest).is_ok_and(|largest| largest <= MAX_RECORD))
            .ok_or(HeaderError::RecordTooLarge)?;
        let session_id = header.terms.session_id();
        let slots_len = header.slots.len();
        let mut slots = Vec::with_capacity(header.slots.len());
        let mut entries = LogEntries::default();
        let mut main_keys = Vec::new();
        for ((slot, start), &plan) in (0..).zip(&header.slots).zip(&header.terms.slots) {
            let slot = PlayerSlot::new(slot);
            if start.plan() != plan {
                return Err(HeaderError::PlanMismatch { slot });
            }
            let control = match start {
                SlotStart::Player(delegation) => {
                    delegation
                        .check(&header.terms.server_key, &session_id)
                        .map_err(|error| HeaderError::Scope { slot, error })?;
                    if main_keys.contains(delegation.main_key()) {
                        return Err(HeaderError::MainKeyRepeated { slot });
                    }
                    main_keys.push(*delegation.main_key());
                    let index = entries.push_delegation((**delegation).clone());
                    Control::Player {
                        delegation: index,
                        chain: InputChain::new(slot, delegation.chain_root()),
                    }
                }
                SlotStart::Bot => Control::Bot,
                SlotStart::Open => Control::Open,
            };
            slots.push(Slot::of(control));
        }
        let mut log = Self {
            entries,
            control: slots.into_iter().collect(),
            schedule: InputSchedule::new(slots_len, header.terms.inputs),
            segments: Segments::default(),
            loaded: false,
            result: None,
            header,
            revealed: None,
            verifier: Verifier::new(),
            signatures: Signatures::Check,
            message: Vec::new(),
            server_since: 0,
            changes: Vec::new(),
            sealed_changes: 0..0,
            to_replay: PagedVec::default(),
            bounds,
            file_len: 0,
            unit: largest + RECORD_SLACK,
            session_id,
            journal: None,
            durable: DurableHeads::new(slots_len),
        };
        // The tag and the header, then a byte each: one segment, of no tick, no entry since, and
        // neither the seed nor the result.
        let mut start = file::LOG_TAG.to_vec();
        log.put_header(&mut start);
        log.file_len = len(start.len()) + 5;
        Ok(log)
    }

    /// Whether the log may refuse its next entry: its file or its journal has no room for one
    /// more record of the most bytes and the session's end after it. A log that refused an entry
    /// is full. Its server ends the session at the boundary where it finds the log full, which the
    /// room the log keeps lets it do.
    pub fn full(&self) -> bool {
        !self.has_room(Growth {
            file: self.unit,
            journal: self.unit,
        })
    }

    /// Whether the file and the journal take `growth` and keep the room the session's end takes
    /// after it, as `kept` counts it. A decode of the file, which records a checkpoint's record as
    /// its segment starts, then takes every entry the log took.
    fn has_room(&self, growth: Growth) -> bool {
        let kept = self.kept();
        let fits = |len: u64, more: u64, bound: u64| {
            len.checked_add(more)
                .and_then(|len| len.checked_add(kept))
                .is_some_and(|len| len <= bound)
        };
        fits(self.file_len, growth.file, self.bounds.file())
            && self
                .journal
                .as_ref()
                .is_none_or(|journal| fits(journal.len(), growth.journal, self.bounds.journal()))
    }

    /// The bytes the session's end may still take: `END_RECORDS` records, and the record of a
    /// checkpoint begun that has none yet.
    fn kept(&self) -> u64 {
        self.unit * (END_RECORDS + u64::from(self.segments.begun().is_some()))
    }

    /// What an entry of `body` bytes adds: to the file, its kind's byte, its body, and the
    /// growth of the count of the entries since the last tick; to the journal, its frame, its
    /// kind's byte and its body.
    fn entry_growth(&self, body: u64) -> Growth {
        let since = u64::from(self.entries.since_sealed());
        Growth {
            file: 1 + body + varint_len(since + 1) - varint_len(since),
            journal: len(journal_frames::FRAME_OVERHEAD) + 1 + body,
        }
    }

    /// Notes the entry the log took last, of `growth`, in the file's length, and writes it into
    /// the journal when the log keeps one.
    fn took_entry(&mut self, growth: Growth) {
        self.file_len += growth.file;
        debug_assert!(
            self.file_len <= self.bounds.file(),
            "the file stays within its bound"
        );
        let before = self.journal.as_ref().map(Journal::len);
        self.journal_last_entry();
        debug_assert_eq!(
            self.journal.as_ref().map(Journal::len),
            before.map(|before| before + growth.journal),
            "an entry's record adds what the log counted"
        );
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
        self.revealed
            .map(|seed| ServerSeeds::new(self.segment(), seed))
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
    pub const fn loaded(&self) -> bool {
        self.loaded
    }

    /// The checkpoint the log began and has no record of yet.
    pub const fn begun_checkpoint(&self) -> Option<&CheckpointBegun> {
        self.segments.begun()
    }

    /// Every checkpoint record, by segment from the second.
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

    /// The last stamp of the inputs `slot`'s player sent since it joined, below which the log
    /// refuses the next; none before its first, or for a slot the session does not have.
    pub fn last_stamp(&self, slot: PlayerSlot) -> Option<Tick> {
        self.debug_at_end();
        self.control.last_stamp(slot)
    }

    /// Who controls `slot` now; none for a slot the session does not have.
    pub fn controller(&self, slot: PlayerSlot) -> Option<Controller> {
        self.debug_at_end();
        self.control.controller(slot, &self.entries)
    }

    /// The main key of the player who left `slot` last, while no player took it since.
    pub fn leaver(&self, slot: PlayerSlot) -> Option<MainKey> {
        self.debug_at_end();
        self.control.leaver(slot)
    }

    /// Checks that the log does not replay: its slots' control is that of its end, which a tick
    /// the replay reaches does not have.
    fn debug_at_end(&self) {
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays holds its slots' control at its end alone"
        );
    }

    /// How many slots the session has.
    pub fn slot_count(&self) -> u32 {
        offset(self.control.len())
    }

    /// The slots a player controls now, in order.
    pub fn player_slots(&self) -> impl Iterator<Item = PlayerSlot> {
        self.control.player_slots()
    }

    /// Every change of a slot's controller logged so far, in the order logged.
    pub fn changes(&self) -> &[SlotChange] {
        &self.changes
    }

    /// The changes of a slot's controller that apply in the tick last sealed.
    pub fn sealed_changes(&self) -> &[SlotChange] {
        &self.changes[self.sealed_changes.clone()]
    }

    /// Logs a player's packet before the next tick. Writes when each input applies into
    /// `applied`, which is cleared first:
    /// late when the next tick is more than the max input delay after its stamp; early when its
    /// stamp is more than the max input lead ahead; else at `max(stamp, next tick)`, or later,
    /// when the player's inputs before it fill that tick: a player's inputs fill each tick up to
    /// the max inputs per tick, in chain order, and the rest spill to the next. A packet is
    /// refused for what its client controls alone, so one that follows the rules is never
    /// refused, however the network groups its packets: more inputs than the max inputs per
    /// tick, in the packet or of one stamp; a stamp before the one before it; more inputs in all
    /// than the ticks up to the max input lead hold; a payload past the max length; or a bad
    /// signature; and for a slot its player does not control now, or when the log's file or
    /// journal has no room for it and the session's end. Each count and length is checked before
    /// any input is hashed. A refused packet leaves the log unchanged.
    pub fn record<'a, I>(
        &mut self,
        packet: PlayerPacket<'_, I>,
        applied: &mut Vec<Applied>,
    ) -> Result<(), InputError>
    where
        I: IntoIterator<Item = (Tick, &'a [u8])>,
        I::IntoIter: Clone,
    {
        let PlayerPacket {
            slot,
            inputs,
            signature,
        } = packet;
        let inputs = inputs.into_iter();
        if inputs.clone().next().is_none() {
            return Err(InputError::EmptyPacket);
        }
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays takes no input"
        );
        debug_assert!(self.result.is_none(), "a session that ended takes no input");
        let terms = &self.header.terms;
        let state = *self.control.get(slot).ok_or(InputError::UnknownPlayer)?;
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
        if stamps.total > ticks.saturating_mul(u64::from(terms.inputs.max_per_tick)) {
            return Err(InputError::AheadOfTime);
        }
        self.message.clear();
        let written = PlayerPacket {
            slot,
            inputs: inputs.clone(),
            signature,
        };
        file::write_packet(&mut self.message, written);
        let growth = self.entry_growth(len(self.message.len()));
        if !self.has_room(growth) {
            return Err(InputError::LogFull);
        }
        for (stamp, payload) in inputs.clone() {
            chain.extend(stamp, payload);
        }
        let head = chain.signed_head(self.session_id);
        if !self.checks(signature, &session_key, &head) {
            return Err(InputError::BadSignature);
        }

        let taken = TakenPacket {
            delegation,
            chain,
            stamps,
        };
        self.control.took_packet(slot, taken);
        applied.clear();
        applied.reserve_exact(at(count));
        let next = self.next_tick();
        let start = self.entries.inputs_len();
        for (stamp, payload) in inputs {
            let index = self.entries.push_input(PlayerInput {
                slot,
                stamp,
                payload,
            });
            applied.push(self.schedule.schedule(&self.entries, index, next));
        }
        let end = self.entries.inputs_len();
        self.entries.push_packet(start..end, *signature);
        self.took_entry(growth);
        if let Some(journal) = &self.journal {
            let head = DurableHead {
                delegation: *self.entries.delegation(delegation).id(),
                seq: chain.next_seq() - 1,
                head: chain.head(),
            };
            self.durable.journaled(journal.written() - 1, slot, head);
        }
        Ok(())
    }

    /// What a server input would do to its slot's controller, logged now, as its structure
    /// allows, before its signature is checked: a join from what the slot is, a leave to what it
    /// becomes, or nothing; an error for an input the log refuses. A bot's commands need a slot a
    /// bot plays, a payload within the max length and a place, as a player's inputs spill past a
    /// tick's max inputs, within the max input lead past the next tick; a join a slot no player
    /// controls, and of a reserved one its leaver, by a main key that controls no other slot; a
    /// renewal, a leave and a link's change a slot a player controls, a renewal by the same main
    /// key; a delegation this session's.
    pub fn change_of(
        &self,
        input: &ServerInput<'_>,
    ) -> Result<Option<SlotChangeKind>, ServerInputError> {
        let terms = &self.header.terms;
        let state = self
            .control
            .get(input.slot())
            .ok_or(ServerInputError::UnknownSlot)?;
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
                let place = self.schedule.place(input.slot(), next);
                if place
                    .since(next)
                    .is_some_and(|lead| lead > terms.inputs.max_lead)
                {
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
                Ok(Some(SlotChangeKind::Joined { from }))
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
                Ok(Some(SlotChangeKind::Left { becomes: *becomes }))
            }
            ServerInput::Connected { .. } | ServerInput::Disconnected { .. } => {
                player_key.ok_or(ServerInputError::NotPlayer)?;
                Ok(None)
            }
        }
    }

    /// Logs a server input before the next tick, with the server key's `signature` over it at its
    /// place, the next server input logged there: what `change_of` allows, and the log's file and
    /// journal have room for, a bot's commands applying in the next tick in slot order, among the
    /// players' inputs, or, past the tick's max inputs, in the first later tick its slot's inputs
    /// leave room in. A join or a leave of a slot drops the inputs its player logged that were
    /// still to apply, and changes the controller from the next tick on. A refused input leaves the
    /// log unchanged.
    pub fn record_server(
        &mut self,
        input: ServerInput<'_>,
        signature: &Signature,
    ) -> Result<(), ServerInputError> {
        let allowed = self.server_allowed(&input)?;
        let place = self.next_place();
        let server_key = &self.header.terms.server_key;
        let id = self.session_id;
        let placed = PlacedInput {
            input: &input,
            session_id: id,
            place,
        };
        let server_key = *server_key;
        if !self.checks(signature, &server_key, &placed) {
            return Err(ServerInputError::BadSignature);
        }
        self.log_server(input, allowed, signature);
        Ok(())
    }

    /// Logs a server input as `record_server` does, signed here by `server`, of the terms'
    /// server key. A signature the log made over the message it wrote needs no check, which is
    /// the most a server input costs.
    pub fn serve(
        &mut self,
        input: ServerInput<'_>,
        server: &Signer,
    ) -> Result<(), ServerInputError> {
        self.debug_server(server);
        let server_key = self.header.terms.server_key;
        let allowed = self.server_allowed(&input)?;
        let placed = PlacedInput {
            input: &input,
            session_id: self.session_id,
            place: self.next_place(),
        };
        let signature = server.sign_with(&placed, &mut self.message);
        debug_assert!(
            self.verifier.verifies(&signature, &server_key, &placed),
            "a signature the log made verifies"
        );
        self.log_server(input, allowed, &signature);
        Ok(())
    }

    /// Whether `signature` is `key`'s over `record`, or trusted as the log's own past.
    fn checks(
        &mut self,
        signature: &Signature,
        key: &XOnlyPublicKey,
        record: &impl Signed,
    ) -> bool {
        self.signatures == Signatures::Trust || self.verifier.verifies(signature, key, record)
    }

    /// Checks that `server` holds the terms' server key, which signs what the server logs.
    fn debug_server(&self, server: &Signer) {
        debug_assert_eq!(
            server.public_key(),
            self.header.terms.server_key,
            "the server signs with the terms' server key"
        );
    }

    /// What `input` does, when `change_of` allows it and the file and the journal have room for
    /// it with its signature.
    fn server_allowed(
        &mut self,
        input: &ServerInput<'_>,
    ) -> Result<ServerAllowed, ServerInputError> {
        let change = self.change_of(input)?;
        self.message.clear();
        input.encode(&mut self.message);
        let growth = self.entry_growth(len(self.message.len()) + Signature::ENCODED_LEN);
        if !self.has_room(growth) {
            return Err(ServerInputError::LogFull);
        }
        Ok(ServerAllowed { change, growth })
    }

    /// Logs a server input `server_allowed` let through, with its `signature`.
    fn log_server(
        &mut self,
        input: ServerInput<'_>,
        allowed: ServerAllowed,
        signature: &Signature,
    ) {
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays takes no input"
        );
        debug_assert!(self.result.is_none(), "a session that ended takes no input");
        let slot = input.slot();
        let next = self.next_tick();
        let served = match input {
            ServerInput::Bot { payload, .. } => {
                let index = self.entries.push_input(PlayerInput {
                    slot,
                    stamp: next,
                    payload,
                });
                self.schedule.schedule(&self.entries, index, next);
                Served::Bot { input: index }
            }
            ServerInput::Join { delegation, .. } => {
                let chain = InputChain::joined(slot, delegation.id(), self.next_place());
                let index = self.entries.push_delegation(delegation);
                self.control.join(slot, index, chain);
                self.schedule.forget(slot);
                self.durable.forget(slot);
                Served::Join {
                    slot,
                    delegation: index,
                }
            }
            // The chain's head stays the old key's until the new key signs an input: a receipt
            // names the key that signed its head, which the player's client then no longer holds.
            ServerInput::Renew { delegation, .. } => {
                let index = self.entries.push_delegation(delegation);
                self.control.renew(slot, index);
                self.durable.forget(slot);
                Served::Renew {
                    slot,
                    delegation: index,
                }
            }
            ServerInput::Leave {
                reason, becomes, ..
            } => {
                self.control.leave(slot, becomes, &self.entries);
                self.schedule.forget(slot);
                self.durable.forget(slot);
                Served::Leave {
                    slot,
                    reason,
                    becomes,
                }
            }
            ServerInput::Connected { .. } => Served::Connected { slot },
            ServerInput::Disconnected { .. } => Served::Disconnected { slot },
        };
        if let Some(kind) = allowed.change {
            self.changes.push(SlotChange {
                tick: next,
                slot,
                kind,
            });
        }
        self.entries.push_server(served, *signature);
        self.server_since += 1;
        self.took_entry(allowed.growth);
    }

    /// Ends the last segment at the boundary before the next tick, and starts the next there, as
    /// a checkpoint begins: from then on the sim draws from the new segment's seed. Its record,
    /// which the server signs once its snapshot is written, comes later by `record_checkpoint`;
    /// until it does, the log begins no other. The last segment must hold a tick. The segment,
    /// the tick, and the log's own state there, which the record must carry. A checkpoint comes
    /// at a tick boundary, before any entry of the tick after it.
    pub fn begin_checkpoint(&mut self) -> Result<CheckpointBegun, CheckpointError> {
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays begins no checkpoint"
        );
        assert!(
            self.result.is_none(),
            "a session that ended takes no checkpoint"
        );
        assert!(
            self.at_boundary(),
            "a checkpoint comes before any entry of its first tick"
        );
        if self.segments.begun().is_some() {
            return Err(CheckpointError::Pending);
        }
        let tick = self.next_tick();
        if self.segments.last_start().tick == tick {
            return Err(CheckpointError::Empty);
        }
        let number = offset(self.segments.len());
        // The count of segments grows, and the new one's count of ticks, 0, takes a byte.
        self.file_len += varint_len(u64::from(number) + 1) - varint_len(u64::from(number)) + 1;
        let begun = CheckpointBegun {
            segment: number,
            tick,
            carry: self.carry(),
        };
        self.segments.begin(begun.clone());
        if let Some(journal) = &mut self.journal {
            self.segments.last_start().journal_begun(journal, number);
        }
        Ok(begun)
    }

    /// Logs `record`, with the server key's `signature` over it, as the record of the checkpoint
    /// the log began and has none of yet, as many ticks after its boundary as it took: it must
    /// start that segment, at its tick, and carry the log's own state there, which a segment's
    /// verification from its checkpoint alone will start from. A refused record leaves the log
    /// unchanged.
    pub fn record_checkpoint(
        &mut self,
        record: Checkpoint,
        signature: &Signature,
    ) -> Result<(), CheckpointError> {
        self.segments.begun().ok_or(CheckpointError::NotBegun)?;
        let server_key = &self.header.terms.server_key;
        let id = self.session_id;
        let signed = InSession {
            session_id: id,
            record: &record,
        };
        let server_key = *server_key;
        if !self.checks(signature, &server_key, &signed) {
            return Err(CheckpointError::BadSignature);
        }
        self.take_checkpoint(record, signature)
    }

    /// Logs `record` as `record_checkpoint` does, signed here by `server`, of the terms' server
    /// key, whose signature over the record the log made needs no check.
    pub fn serve_checkpoint(
        &mut self,
        record: Checkpoint,
        server: &Signer,
    ) -> Result<(), CheckpointError> {
        self.debug_server(server);
        let signature = server.sign_with(
            &InSession {
                session_id: self.session_id,
                record: &record,
            },
            &mut self.message,
        );
        self.take_checkpoint(record, &signature)
    }

    /// Logs `record`, whose `signature` is the server's, as the record of the checkpoint begun.
    fn take_checkpoint(
        &mut self,
        record: Checkpoint,
        signature: &Signature,
    ) -> Result<(), CheckpointError> {
        let begun = self.segments.begun().ok_or(CheckpointError::NotBegun)?;
        if record.segment != begun.segment {
            return Err(CheckpointError::Segment);
        }
        if record.tick != begun.tick {
            return Err(CheckpointError::Tick);
        }
        if record.carry != begun.carry {
            return Err(CheckpointError::Carry);
        }
        self.message.clear();
        record.encode(&mut self.message);
        self.file_len += len(self.message.len()) + Signature::ENCODED_LEN;
        debug_assert!(
            self.file_len <= self.bounds.file(),
            "a checkpoint has its room"
        );
        let logged = LoggedCheckpoint {
            record,
            signature: *signature,
        };
        if let Some(journal) = &mut self.journal {
            logged.journal(journal);
        }
        self.segments.record(logged);
        Ok(())
    }

    /// The log's own state at the boundary before the next tick, as a checkpoint there carries
    /// it.
    fn carry(&self) -> LogCarry {
        self.debug_at_end();
        let slots = (0..)
            .map(PlayerSlot::new)
            .zip(self.control.iter())
            .map(|(at, slot)| CarriedSlot {
                control: match slot.control {
                    Control::Player { delegation, chain } => CarriedControl::Player {
                        delegation: Box::new(self.entries.delegation(delegation).clone()),
                        chain,
                    },
                    Control::Bot => CarriedControl::Bot,
                    Control::Open => CarriedControl::Open,
                    Control::Reserved => CarriedControl::Reserved,
                },
                leaver: slot.leaver,
                stamps: slot.stamps,
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
                    slot: due.slot,
                    stamp: input.stamp,
                    payload: input.payload.to_vec(),
                }
            })
            .collect();
        LogCarry { slots, pending }
    }

    /// Ends the session with `result`, with the server key's `signature` over it, which must
    /// stop before the next tick. A refused result leaves the log unchanged.
    pub fn record_result(
        &mut self,
        result: SessionResult,
        signature: &Signature,
    ) -> Result<(), ResultError> {
        let server_key = &self.header.terms.server_key;
        let id = self.session_id;
        let signed = InSession {
            session_id: id,
            record: &result,
        };
        let server_key = *server_key;
        if !self.checks(signature, &server_key, &signed) {
            return Err(ResultError::BadSignature);
        }
        self.take_result(result, signature)
    }

    /// Ends the session with `result` as `record_result` does, signed here by `server`, of the
    /// terms' server key, whose signature over the result the log made needs no check.
    pub fn serve_result(
        &mut self,
        result: SessionResult,
        server: &Signer,
    ) -> Result<(), ResultError> {
        self.debug_server(server);
        let signature = server.sign_with(
            &InSession {
                session_id: self.session_id,
                record: &result,
            },
            &mut self.message,
        );
        self.take_result(result, &signature)
    }

    /// Ends the session with `result`, whose `signature` is the server's.
    fn take_result(
        &mut self,
        result: SessionResult,
        signature: &Signature,
    ) -> Result<(), ResultError> {
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays takes no result"
        );
        assert!(self.result.is_none(), "a session ends once");
        if result.tick != self.next_tick() {
            return Err(ResultError::Tick);
        }
        let logged = LoggedResult {
            result,
            signature: *signature,
        };
        if let Some(journal) = &mut self.journal {
            logged.journal(journal);
        }
        self.result = Some(logged);
        self.message.clear();
        put(&mut self.message, &self.result);
        self.file_len += len(self.message.len()) - 1;
        debug_assert!(self.file_len <= self.bounds.file(), "the end has its room");
        Ok(())
    }

    /// Closes the next tick to new entries and gives the inputs applied in it, by slot, then in
    /// the order logged; `sealed_changes` then gives the changes of a slot's controller in it.
    pub fn seal_tick(&mut self) -> impl ExactSizeIterator<Item = PlayerInput<'_>> {
        self.seal();
        self.schedule
            .due()
            .iter()
            .map(|&index| self.entries.input(index))
    }

    /// Seals each tick before `tick`, the log's own state only, as a replay does that a
    /// checkpoint's snapshot starts at `tick`.
    pub fn seal_until(&mut self, tick: Tick) {
        while self.next_tick() < tick {
            self.seal();
        }
    }

    /// Closes the next tick to new entries, and has the schedule give the inputs applied in it.
    fn seal(&mut self) {
        let tick = self.next_tick();
        if self.to_replay.is_empty() {
            // The segment's count of ticks grows, and the next tick's count of entries, 0,
            // takes a byte.
            let ticks = tick.get() - self.segments.last_start().tick.get();
            self.file_len += varint_len(ticks + 1) - varint_len(ticks) + 1;
            debug_assert!(self.file_len <= self.bounds.file(), "a seal has its room");
            if let Some(journal) = &mut self.journal {
                journal_records::sealed(journal);
            }
        }
        let replayed = usize::try_from(tick.get()).expect("tick fits usize");
        let end = match self.to_replay.get(replayed) {
            Some(&end) => {
                for at in self.entries.sealed_end()..end {
                    self.schedule.replay(&self.entries, at, tick);
                }
                end
            }
            None => self.entries.len(),
        };
        self.entries.seal(end);
        self.server_since = 0;
        if !self.to_replay.is_empty() && self.entries.next_tick().get() == len(self.to_replay.len())
        {
            // Caught up: the entries logged after the last tick wait again, as they did.
            self.to_replay = PagedVec::default();
            let next = self.next_tick();
            for at in end..self.entries.len() {
                if self.schedule.replay(&self.entries, at, next) {
                    self.server_since += 1;
                }
            }
        }
        let first = self.changes.partition_point(|change| change.tick < tick);
        let after = self.changes.partition_point(|change| change.tick <= tick);
        self.sealed_changes = first..after;
        self.schedule.seal(tick);
    }

    /// This log with its ticks unsealed, to replay them: sealing each again gives the inputs it
    /// applied, from what the log holds, with no check done twice. Once the last is sealed, the
    /// log is as it was. A log that replays takes no entry.
    #[must_use]
    pub fn rewound(mut self) -> Self {
        if self.entries.next_tick().get() == 0 {
            return self;
        }
        self.to_replay = self.entries.take_tick_ends();
        self.schedule.reset();
        self
    }

    /// Goes back to the boundary where segment `segment` starts, its checkpoint's, as a load of
    /// that save does: the log becomes what it held there, its segments after that one and every
    /// entry logged after the boundary dropped, and the segment goes on from its checkpoint, with
    /// its seed. The journal logs the load, so a log rebuilt from it is the log loaded. An error
    /// when the log holds no checkpoint that starts the segment, and when its journal has no room
    /// for the load's record.
    pub fn load(&mut self, segment: u32) -> Result<(), LogLoadError> {
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays loads nothing"
        );
        if self
            .segments
            .get(segment)
            .is_none_or(|start| start.checkpoint.is_none())
        {
            return Err(LogLoadError::NoCheckpoint);
        }
        let record = Growth {
            file: 0,
            journal: len(journal_frames::FRAME_OVERHEAD) + 1 + varint_len(u64::from(segment)),
        };
        if !self.has_room(record) {
            return Err(LogLoadError::LogFull);
        }
        let mut bytes = Vec::new();
        self.encode_through(at(segment), &mut bytes);
        let mut loaded = Self::decode_within(&bytes, self.bounds, Signatures::Trust)
            .expect("the log's own past decodes");
        if let Some(mut journal) = self.journal.take() {
            journal_records::loaded(&mut journal, segment);
            loaded.journal = Some(journal);
        }
        loaded.loaded = true;
        *self = loaded;
        Ok(())
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
        error @ BinaryError::Malformed(_) => LogError::Malformed(error),
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
            debug_assert!(self.full(), "the log is full");
        }
    }
}

#[cfg(test)]
mod tests;
