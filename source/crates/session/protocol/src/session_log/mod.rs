use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::mem;
use std::ops::Range;

use campfire_codec::{Binary, BinaryError};
use campfire_common::{AfterLeave, PlayerSlot, Tick};
use serde::{Deserialize, Serialize};

use crate::checkpoint::Checkpoint;
use crate::checkpoint::checkpoint_begun::CheckpointBegun;
use crate::checkpoint::log_carry::{CarriedControl, CarriedInput, CarriedSlot, LogCarry};
use crate::controller::Controller;
use crate::delegation::Delegation;
use crate::input_chain::InputChain;
use crate::journal::Journal;
use crate::journal::journal_frames::{JournalFrames, MAX_RECORD};
use crate::main_key::MainKey;
use crate::player_input::PlayerInput;
use crate::server_input::{InputPlace, LeaveReason, ServerInput};
use crate::server_seed::ServerSeed;
use crate::server_seeds::ServerSeeds;
use crate::session_id::SessionId;
use crate::session_log::applied::Applied;
use crate::session_log::durable_head::DurableHead;
use crate::session_log::durable_heads::DurableHeads;
use crate::session_log::error::{
    CheckpointError, HeaderError, InputError, LogError, LogLoadError, ResultError, ServerInputError,
};
use crate::session_log::paged_bytes::{BytesAt, PagedBytes};
use crate::session_log::paged_vec::PagedVec;
use crate::session_log::session_header::SessionHeader;
use crate::session_result::SessionResult;
use crate::signature::Signature;
use crate::signer::Signer;
use crate::slot_change::{SlotChange, SlotChangeKind, Taken};
use crate::slot_start::SlotStart;
use crate::spill::Spill;
use crate::stamp_count::StampCount;
use crate::verifier::Verifier;

pub(crate) mod applied;
pub(crate) mod durable_head;
mod durable_heads;
pub(crate) mod error;
mod file;
mod journal_records;
pub(crate) mod paged_bytes;
pub(crate) mod paged_vec;
pub(crate) mod session_header;

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
    /// The scratch a server input's message is written into as the log signs it.
    message: Vec<u8>,
    /// Each slot's controller and counts, by slot.
    slots: Vec<Slot>,
    /// Every delegation a player held, the header's and the joins' and renewals', which a
    /// player's controller names by index.
    delegations: Vec<Delegation>,
    /// Each segment's start, in order: the first at tick 0, every other at its checkpoint.
    segments: Vec<SegmentStart>,
    /// The checkpoint that starts the last segment, while its record has not come.
    begun: Option<CheckpointBegun>,
    /// Whether the log went back to a save: a load it took, or one in the journal it was
    /// rebuilt from.
    loaded: bool,
    /// How the session ended, once it did.
    result: Option<LoggedResult>,
    /// The whole session's inputs, payloads, packets, server inputs, entries and tick ends grow
    /// by pages, so a record never copies the history before it.
    inputs: PagedVec<LoggedInput>,
    payloads: PagedBytes,
    packets: PagedVec<Packet>,
    server: PagedVec<LoggedServer>,
    /// The packets and the server inputs, in the order they were logged.
    entries: PagedVec<Entry>,
    /// The server inputs logged since the last tick was sealed.
    server_since: u32,
    /// Every change of a slot's controller, in the order logged, so by tick.
    changes: Vec<SlotChange>,
    /// The changes of the tick last sealed.
    sealed_changes: Range<usize>,
    /// For each sealed tick, the end of the entries logged before it ran.
    tick_ends: PagedVec<u32>,
    /// While a rewound log replays, the tick ends it had; empty otherwise.
    to_replay: PagedVec<u32>,
    /// Inputs applied in a tick not yet sealed, earliest first.
    pending: BinaryHeap<Reverse<Due>>,
    /// Indices of the inputs applied in the tick last sealed.
    due: Vec<u32>,
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

/// The most bytes a log's file and its journal hold: `MAX_FILE_LEN` and `MAX_JOURNAL_LEN`, or
/// less in a test.
#[derive(Debug, Clone, Copy)]
struct Bounds {
    file: u64,
    journal: u64,
}

/// The bytes a record adds to the log's file and to its journal.
#[derive(Debug, Clone, Copy)]
struct Growth {
    file: u64,
    journal: u64,
}

/// A slot as the log follows it: who controls it, the main key of the player who left it last,
/// and its counts. Its spill counts its controller's inputs alone: a change of controller starts
/// it afresh, so in a bot's slot it counts the bot's.
#[derive(Debug, Clone, Copy)]
struct Slot {
    control: Control,
    leaver: Option<MainKey>,
    stamps: StampCount,
    spill: Spill,
    /// The delegation whose key signed the head of its player's chain, by index; none before
    /// their first input.
    head_signer: Option<u32>,
}

/// Who controls a slot.
#[derive(Debug, Clone, Copy)]
enum Control {
    /// A player, by the index of their current delegation, with their chain as logged so far.
    Player {
        delegation: u32,
        chain: InputChain,
    },
    Bot,
    Open,
    /// None: it waits for the player who left it.
    Reserved,
}

#[derive(Debug)]
struct LoggedInput {
    slot: PlayerSlot,
    stamp: Tick,
    payload: BytesAt,
}

/// A player's packet: its inputs, and the signature over the chain head after the last.
#[derive(Debug)]
struct Packet {
    inputs: Range<u32>,
    signature: Signature,
}

/// A server input as logged, with its signature.
#[derive(Debug)]
struct LoggedServer {
    served: Served,
    signature: Signature,
}

/// A logged server input, as the log holds it: a bot's commands by the input they are, and a
/// delegation by its index among the log's, so the log keeps each payload and delegation once.
#[derive(Debug, Clone, Copy)]
enum Served {
    Bot {
        input: u32,
    },
    Join {
        slot: PlayerSlot,
        delegation: u32,
    },
    Renew {
        slot: PlayerSlot,
        delegation: u32,
    },
    Leave {
        slot: PlayerSlot,
        reason: LeaveReason,
        becomes: AfterLeave,
    },
    Connected {
        slot: PlayerSlot,
    },
    Disconnected {
        slot: PlayerSlot,
    },
}

/// Where a segment starts: the first tick it holds, and the checkpoint it starts from, which
/// every segment but the first has once its record came.
#[derive(Debug)]
struct SegmentStart {
    tick: Tick,
    checkpoint: Option<LoggedCheckpoint>,
}

#[derive(Debug)]
struct LoggedCheckpoint {
    record: Checkpoint,
    signature: Signature,
}

/// The result and the server's signature over it, as the file holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct LoggedResult {
    result: SessionResult,
    signature: Signature,
}

/// A logged entry, by its place among the packets or the server inputs.
#[derive(Debug, Clone, Copy)]
enum Entry {
    Packet(u32),
    Server(u32),
}

/// Orders the inputs of one tick by slot, then by when they were logged, which for one player is
/// their chain's order, whatever order the players' inputs arrived in, so the host cannot choose
/// who acts first.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Due {
    tick: Tick,
    slot: PlayerSlot,
    index: u32,
}

impl Slot {
    /// A slot of `control` with no input yet.
    fn of(control: Control) -> Self {
        Self {
            control,
            leaver: None,
            stamps: StampCount::default(),
            spill: Spill::default(),
            head_signer: None,
        }
    }
}

impl Bounds {
    const MAX: Self = Self {
        file: SessionLog::MAX_FILE_LEN as u64,
        journal: SessionLog::MAX_JOURNAL_LEN as u64,
    };
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
        let mut delegations = Vec::new();
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
                    if delegations
                        .iter()
                        .any(|earlier: &Delegation| earlier.main_key() == delegation.main_key())
                    {
                        return Err(HeaderError::MainKeyRepeated { slot });
                    }
                    let index = offset(delegations.len());
                    delegations.push((**delegation).clone());
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
            slots,
            delegations,
            segments: vec![SegmentStart {
                tick: Tick::new(0),
                checkpoint: None,
            }],
            begun: None,
            loaded: false,
            result: None,
            header,
            revealed: None,
            verifier: Verifier::new(),
            message: Vec::new(),
            inputs: PagedVec::default(),
            payloads: PagedBytes::default(),
            packets: PagedVec::default(),
            server: PagedVec::default(),
            entries: PagedVec::default(),
            server_since: 0,
            changes: Vec::new(),
            sealed_changes: 0..0,
            tick_ends: PagedVec::default(),
            to_replay: PagedVec::default(),
            pending: BinaryHeap::new(),
            due: Vec::new(),
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
    /// after it, as `kept` counts it. A decode of the file, which records a checkpoint's record as its segment starts, then
    /// takes every entry the log took.
    fn has_room(&self, growth: Growth) -> bool {
        let kept = self.kept();
        let fits = |len: u64, more: u64, bound: u64| {
            len.checked_add(more)
                .and_then(|len| len.checked_add(kept))
                .is_some_and(|len| len <= bound)
        };
        fits(self.file_len, growth.file, self.bounds.file)
            && self
                .journal
                .as_ref()
                .is_none_or(|journal| fits(journal.len(), growth.journal, self.bounds.journal))
    }

    /// The bytes the session's end may still take: `END_RECORDS` records, and the record of a
    /// checkpoint begun that has none yet.
    fn kept(&self) -> u64 {
        self.unit * (END_RECORDS + u64::from(self.begun.is_some()))
    }

    /// What an entry of `body` bytes adds: to the file, its kind's byte, its body, and the
    /// growth of the count of the entries since the last tick; to the journal, its frame, its
    /// kind's byte and its body.
    fn entry_growth(&self, body: u64) -> Growth {
        let since =
            len(self.entries.len()) - u64::from(self.tick_ends.last().copied().unwrap_or(0));
        Growth {
            file: 1 + body + varint_len(since + 1) - varint_len(since),
            journal: len(JournalFrames::FRAME_OVERHEAD) + 1 + body,
        }
    }

    /// Notes the entry the log took last, of `growth`, in the file's length, and writes it into
    /// the journal when the log keeps one.
    fn took_entry(&mut self, growth: Growth) {
        self.file_len += growth.file;
        debug_assert!(
            self.file_len <= self.bounds.file,
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
        debug_assert!(self.file_len <= self.bounds.file, "the end has its room");
    }

    /// The last segment's server seed, once the log is published.
    pub const fn revealed_seed(&self) -> Option<ServerSeed> {
        self.revealed
    }

    /// Every segment's server seed, once the log is published.
    pub fn revealed_seeds(&self) -> Option<ServerSeeds> {
        self.revealed
            .map(|seed| ServerSeeds::new(self.segment(), seed))
    }

    /// The last segment, which the next tick runs in.
    pub fn segment(&self) -> u32 {
        offset(self.segments.len() - 1)
    }

    /// The checkpoint record of the segment that starts at `tick`, when one does and its record
    /// came.
    pub fn checkpoint_at(&self, tick: Tick) -> Option<&Checkpoint> {
        let at = self.segment_starting(tick)?;
        let segment = &self.segments[usize::try_from(at).expect("segments fit usize")];
        segment.checkpoint.as_ref().map(|logged| &logged.record)
    }

    /// The segment that starts at `tick`, its record come or not; none at tick 0, where the
    /// first starts from no checkpoint.
    pub fn segment_starting(&self, tick: Tick) -> Option<u32> {
        let at = self.segments.partition_point(|segment| segment.tick < tick);
        self.segments
            .get(at)
            .filter(|segment| at > 0 && segment.tick == tick)?;
        Some(offset(at))
    }

    /// Whether the log went back to a save: a load it took, or one in the journal it was rebuilt
    /// from. A player's chain may then stand before what their client holds.
    pub const fn loaded(&self) -> bool {
        self.loaded
    }

    /// The checkpoint the log began and has no record of yet.
    pub const fn begun_checkpoint(&self) -> Option<&CheckpointBegun> {
        self.begun.as_ref()
    }

    /// Every checkpoint record, by segment from the second.
    pub fn checkpoints(&self) -> impl Iterator<Item = &Checkpoint> {
        self.segments
            .iter()
            .filter_map(|segment| segment.checkpoint.as_ref())
            .map(|logged| &logged.record)
    }

    /// How the session ended, once it did.
    pub fn result(&self) -> Option<&SessionResult> {
        self.result.as_ref().map(|logged| &logged.result)
    }

    /// The tick that the entries recorded now arrive before.
    pub fn next_tick(&self) -> Tick {
        Tick::new(u64::try_from(self.tick_ends.len()).expect("tick count fits u64"))
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
        self.slots.get(slot.index())?.stamps.last
    }

    /// Who controls `slot` now; none for a slot the session does not have.
    pub fn controller(&self, slot: PlayerSlot) -> Option<Controller> {
        let held = self.slots.get(slot.index())?;
        Some(match held.control {
            Control::Player { delegation, chain } => {
                let delegation = &self.delegations[delegation as usize];
                Controller::Player {
                    main_key: *delegation.main_key(),
                    session_key: delegation.terms().session_key,
                    chain,
                }
            }
            Control::Bot => Controller::Bot,
            Control::Open => Controller::Open,
            Control::Reserved => Controller::Reserved,
        })
    }

    /// The main key of the player who left `slot` last, while no player took it since.
    pub fn leaver(&self, slot: PlayerSlot) -> Option<MainKey> {
        self.slots.get(slot.index())?.leaver
    }

    /// How many slots the session has.
    pub fn slot_count(&self) -> u32 {
        offset(self.slots.len())
    }

    /// The slots a player controls now, in order.
    pub fn player_slots(&self) -> impl Iterator<Item = PlayerSlot> {
        (0..)
            .zip(&self.slots)
            .filter(|(_, slot)| matches!(slot.control, Control::Player { .. }))
            .map(|(slot, _)| PlayerSlot::new(slot))
    }

    /// Every server input logged so far, in the order logged.
    pub fn server_inputs(&self) -> impl Iterator<Item = ServerInput<'_>> {
        self.server
            .iter()
            .map(|logged| self.server_input(logged.served))
    }

    /// Every change of a slot's controller logged so far, in the order logged.
    pub fn changes(&self) -> &[SlotChange] {
        &self.changes
    }

    /// The changes of a slot's controller that apply in the tick last sealed.
    pub fn sealed_changes(&self) -> &[SlotChange] {
        &self.changes[self.sealed_changes.clone()]
    }

    /// Logs a packet of the player of `slot` before the next tick: `inputs`, each its stamp and
    /// payload, in the order sent, and the player's session-key `signature` over the chain head
    /// after the last. Writes when each input applies into `applied`, which is cleared first:
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
        slot: PlayerSlot,
        inputs: I,
        signature: &Signature,
        applied: &mut Vec<Applied>,
    ) -> Result<(), InputError>
    where
        I: IntoIterator<Item = (Tick, &'a [u8])>,
        I::IntoIter: Clone,
    {
        let inputs = inputs.into_iter();
        if inputs.clone().next().is_none() {
            return Err(InputError::EmptyPacket);
        }
        let player = slot.index();
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays takes no input"
        );
        debug_assert!(self.result.is_none(), "a session that ended takes no input");
        let terms = &self.header.terms;
        let state = *self.slots.get(player).ok_or(InputError::UnknownPlayer)?;
        let Control::Player {
            delegation,
            mut chain,
        } = state.control
        else {
            return Err(InputError::NotPlayer);
        };
        let session_key = self.delegations[delegation as usize].terms().session_key;
        let mut stamps = state.stamps;
        let mut count = 0;
        for (stamp, payload) in inputs.clone() {
            if payload.len() > terms.inputs.max_payload_len as usize {
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
        file::write_packet(&mut self.message, slot, count, inputs.clone(), signature);
        let growth = self.entry_growth(len(self.message.len()));
        if !self.has_room(growth) {
            return Err(InputError::LogFull);
        }
        let count = count as usize;
        for (stamp, payload) in inputs.clone() {
            chain.extend(stamp, payload);
        }
        if !chain.signed_by(&self.verifier, &session_key, self.session_id, signature) {
            return Err(InputError::BadSignature);
        }

        let held = &mut self.slots[player];
        held.control = Control::Player { delegation, chain };
        held.stamps = stamps;
        held.head_signer = Some(delegation);
        applied.clear();
        applied.reserve_exact(count);
        let start = offset(self.inputs.len());
        for (stamp, payload) in inputs {
            let index = self.push_input(PlayerInput {
                slot,
                stamp,
                payload,
            });
            applied.push(self.schedule(index));
        }
        self.entries.push(Entry::Packet(offset(self.packets.len())));
        self.packets.push(Packet {
            inputs: start..offset(self.inputs.len()),
            signature: *signature,
        });
        self.took_entry(growth);
        if let Some(journal) = &self.journal {
            let head = DurableHead {
                delegation: *self.delegations[delegation as usize].id(),
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
            .slots
            .get(input.slot().index())
            .ok_or(ServerInputError::UnknownSlot)?;
        let scope = |delegation: &Delegation| {
            delegation
                .check(&terms.server_key, &self.session_id)
                .map_err(ServerInputError::Scope)
        };
        let player_key = match state.control {
            Control::Player { delegation, .. } => {
                Some(*self.delegations[delegation as usize].main_key())
            }
            Control::Bot | Control::Open | Control::Reserved => None,
        };
        match input {
            ServerInput::Bot { payload, .. } => {
                if !matches!(state.control, Control::Bot) {
                    return Err(ServerInputError::NotBot);
                }
                if payload.len() > terms.inputs.max_payload_len as usize {
                    return Err(ServerInputError::PayloadTooLarge);
                }
                let next = self.next_tick();
                let place = state.spill.place(next, terms.inputs.max_per_tick);
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
                let main_key = delegation.main_key();
                if self.slots.iter().any(|slot| match slot.control {
                    Control::Player { delegation, .. } => {
                        self.delegations[delegation as usize].main_key() == main_key
                    }
                    Control::Bot | Control::Open | Control::Reserved => false,
                }) {
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

    /// Logs a server input before the next tick, with the server key's `signature` over it at
    /// its place, the next server input logged there: what `change_of` allows, and the log's
    /// file and journal have room for, a bot's commands applying in the next tick in slot order,
    /// among the players' inputs, or, past the tick's max inputs, in the first later tick its
    /// slot's inputs leave room in. A join or a leave of
    /// a slot drops the inputs its player logged that were still to apply, and changes the
    /// controller from the next tick on. A refused input leaves the log unchanged.
    pub fn record_server(
        &mut self,
        input: ServerInput<'_>,
        signature: &Signature,
    ) -> Result<(), ServerInputError> {
        let change = self.change_of(&input)?;
        let growth = self.server_growth(&input)?;
        let place = self.next_place();
        let server_key = &self.header.terms.server_key;
        let id = self.session_id;
        if !input.signed_by(&mut self.verifier, server_key, id, place, signature) {
            return Err(ServerInputError::BadSignature);
        }
        self.log_server(input, change, growth, signature);
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
        let server_key = self.header.terms.server_key;
        debug_assert_eq!(
            server.public_key(),
            server_key,
            "the server signs with the terms' server key"
        );
        let change = self.change_of(&input)?;
        let growth = self.server_growth(&input)?;
        let place = self.next_place();
        input.write_message(self.session_id, place, &mut self.message);
        let signature = server.sign(&self.message);
        debug_assert!(
            self.verifier
                .verifies(signature, &server_key, &self.message),
            "a signature the log made verifies"
        );
        self.log_server(input, change, growth, &signature);
        Ok(())
    }

    /// What `input` adds to the file and the journal, with its signature; an error when they
    /// have no room for it.
    fn server_growth(&mut self, input: &ServerInput<'_>) -> Result<Growth, ServerInputError> {
        self.message.clear();
        input.encode(&mut self.message);
        let growth = self.entry_growth(len(self.message.len()) + Signature::ENCODED_LEN);
        if !self.has_room(growth) {
            return Err(ServerInputError::LogFull);
        }
        Ok(growth)
    }

    /// Logs a server input `change_of` allowed, of `growth`, with its `signature`.
    fn log_server(
        &mut self,
        input: ServerInput<'_>,
        change: Option<SlotChangeKind>,
        growth: Growth,
        signature: &Signature,
    ) {
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays takes no input"
        );
        debug_assert!(self.result.is_none(), "a session that ended takes no input");
        let slot = input.slot();
        let at = slot.index();
        let next = self.next_tick();
        let served = match input {
            ServerInput::Bot { payload, .. } => {
                let index = self.push_input(PlayerInput {
                    slot,
                    stamp: next,
                    payload,
                });
                self.schedule(index);
                Served::Bot { input: index }
            }
            ServerInput::Join { delegation, .. } => {
                let chain = InputChain::joined(slot, delegation.id(), self.next_place());
                let index = self.push_delegation(delegation);
                let control = Control::Player {
                    delegation: index,
                    chain,
                };
                let held = &mut self.slots[at];
                held.control = control;
                held.leaver = None;
                held.stamps = StampCount::default();
                held.head_signer = None;
                self.forget_pending(slot);
                self.durable.forget(slot);
                Served::Join {
                    slot,
                    delegation: index,
                }
            }
            // The chain's head stays the old key's until the new key signs an input: a receipt
            // names the key that signed its head, which the player's client then no longer holds.
            ServerInput::Renew { delegation, .. } => {
                let index = self.push_delegation(delegation);
                if let Control::Player { delegation, .. } = &mut self.slots[at].control {
                    *delegation = index;
                }
                self.durable.forget(slot);
                Served::Renew {
                    slot,
                    delegation: index,
                }
            }
            ServerInput::Leave {
                reason, becomes, ..
            } => {
                let held = &mut self.slots[at];
                if let Control::Player { delegation, .. } = held.control {
                    held.leaver = Some(*self.delegations[delegation as usize].main_key());
                }
                held.control = match becomes {
                    AfterLeave::Reserve => Control::Reserved,
                    AfterLeave::Bot => Control::Bot,
                    AfterLeave::Open => Control::Open,
                };
                held.stamps = StampCount::default();
                self.forget_pending(slot);
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
        if let Some(kind) = change {
            self.changes.push(SlotChange {
                tick: next,
                slot,
                kind,
            });
        }
        self.entries.push(Entry::Server(offset(self.server.len())));
        self.server.push(LoggedServer {
            served,
            signature: *signature,
        });
        self.server_since += 1;
        self.took_entry(growth);
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
        if self.begun.is_some() {
            return Err(CheckpointError::Pending);
        }
        let tick = self.next_tick();
        if self.segments.last().is_some_and(|last| last.tick == tick) {
            return Err(CheckpointError::Empty);
        }
        let segment = SegmentStart {
            tick,
            checkpoint: None,
        };
        let number = offset(self.segments.len());
        // The count of segments grows, and the new one's count of ticks, 0, takes a byte.
        self.file_len += varint_len(u64::from(number) + 1) - varint_len(u64::from(number)) + 1;
        if let Some(journal) = &mut self.journal {
            segment.journal_begun(journal, number);
        }
        self.segments.push(segment);
        let begun = CheckpointBegun {
            segment: number,
            tick,
            carry: self.carry(),
        };
        self.begun = Some(begun.clone());
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
        let begun = self.begun.as_ref().ok_or(CheckpointError::NotBegun)?;
        let server_key = &self.header.terms.server_key;
        let id = self.session_id;
        if !record.signed_by(&mut self.verifier, server_key, id, signature) {
            return Err(CheckpointError::BadSignature);
        }
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
            self.file_len <= self.bounds.file,
            "a checkpoint has its room"
        );
        let logged = LoggedCheckpoint {
            record,
            signature: *signature,
        };
        if let Some(journal) = &mut self.journal {
            logged.journal(journal);
        }
        let segment = usize::try_from(begun.segment).expect("segments fit usize");
        self.segments[segment].checkpoint = Some(logged);
        self.begun = None;
        Ok(())
    }

    /// The log's own state at the boundary before the next tick, as a checkpoint there carries
    /// it.
    pub fn carry(&self) -> LogCarry {
        let slots = self
            .slots
            .iter()
            .map(|slot| CarriedSlot {
                control: match slot.control {
                    Control::Player { delegation, chain } => CarriedControl::Player {
                        delegation: Box::new(self.delegations[delegation as usize].clone()),
                        chain,
                    },
                    Control::Bot => CarriedControl::Bot,
                    Control::Open => CarriedControl::Open,
                    Control::Reserved => CarriedControl::Reserved,
                },
                leaver: slot.leaver,
                stamps: slot.stamps,
                spill: slot.spill,
            })
            .collect();
        let mut due: Vec<&Due> = self.pending.iter().map(|Reverse(due)| due).collect();
        due.sort_unstable();
        let pending = due
            .into_iter()
            .map(|due| {
                let input = self.input(due.index);
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
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays takes no result"
        );
        assert!(self.result.is_none(), "a session ends once");
        let server_key = &self.header.terms.server_key;
        let id = self.session_id;
        if !result.signed_by(&mut self.verifier, server_key, id, signature) {
            return Err(ResultError::BadSignature);
        }
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
        debug_assert!(self.file_len <= self.bounds.file, "the end has its room");
        Ok(())
    }

    /// Closes the next tick to new entries and gives the inputs applied in it, by slot, then in
    /// the order logged; `sealed_changes` then gives the changes of a slot's controller in it.
    pub fn seal_tick(&mut self) -> impl ExactSizeIterator<Item = PlayerInput<'_>> {
        self.seal();
        self.due.iter().map(|&index| self.input(index))
    }

    /// Seals each tick before `tick`, the log's own state only, as a replay does that a
    /// checkpoint's snapshot starts at `tick`.
    pub fn seal_until(&mut self, tick: Tick) {
        while self.next_tick() < tick {
            self.seal();
        }
    }

    /// Closes the next tick to new entries, and puts the inputs applied in it in `due`.
    fn seal(&mut self) {
        let tick = self.next_tick();
        if self.to_replay.is_empty() {
            // The segment's count of ticks grows, and the next tick's count of entries, 0,
            // takes a byte.
            let ticks = tick.get() - self.segments.last().expect("a segment").tick.get();
            self.file_len += varint_len(ticks + 1) - varint_len(ticks) + 1;
            debug_assert!(self.file_len <= self.bounds.file, "a seal has its room");
            if let Some(journal) = &mut self.journal {
                journal_records::sealed(journal);
            }
        }
        let replayed = usize::try_from(tick.get()).expect("tick fits usize");
        let end = match self.to_replay.get(replayed) {
            Some(&end) => {
                let start = self.tick_ends.last().copied().unwrap_or(0);
                for at in start..end {
                    self.replay_entry(at);
                }
                end
            }
            None => offset(self.entries.len()),
        };
        self.tick_ends.push(end);
        self.server_since = 0;
        if !self.to_replay.is_empty() && self.tick_ends.len() == self.to_replay.len() {
            // Caught up: the entries logged after the last tick wait again, as they did.
            self.to_replay = PagedVec::default();
            for at in end..offset(self.entries.len()) {
                if self.replay_entry(at) {
                    self.server_since += 1;
                }
            }
        }
        let first = self.changes.partition_point(|change| change.tick < tick);
        let after = self.changes.partition_point(|change| change.tick <= tick);
        self.sealed_changes = first..after;
        self.due.clear();
        while let Some(Reverse(due)) = self.pending.peek()
            && due.tick == tick
        {
            self.due.push(due.index);
            self.pending.pop();
        }
    }

    /// This log with its ticks unsealed, to replay them: sealing each again gives the inputs it
    /// applied, from what the log holds, with no check done twice. Once the last is sealed, the
    /// log is as it was. A log that replays takes no entry.
    #[must_use]
    pub fn rewound(mut self) -> Self {
        if self.tick_ends.is_empty() {
            return self;
        }
        self.to_replay = mem::take(&mut self.tick_ends);
        self.pending.clear();
        self.due.clear();
        for slot in &mut self.slots {
            slot.spill = Spill::default();
        }
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
        let at = usize::try_from(segment).expect("segments fit usize");
        if self
            .segments
            .get(at)
            .is_none_or(|start| start.checkpoint.is_none())
        {
            return Err(LogLoadError::NoCheckpoint);
        }
        let record = Growth {
            file: 0,
            journal: len(JournalFrames::FRAME_OVERHEAD) + 1 + varint_len(u64::from(segment)),
        };
        if !self.has_room(record) {
            return Err(LogLoadError::LogFull);
        }
        let mut bytes = Vec::new();
        self.encode_through(at, &mut bytes);
        let mut loaded =
            Self::decode_within(&bytes, self.bounds).expect("the log's own past decodes");
        if let Some(mut journal) = self.journal.take() {
            journal_records::loaded(&mut journal, segment);
            loaded.journal = Some(journal);
        }
        loaded.loaded = true;
        *self = loaded;
        Ok(())
    }

    /// The server input `served` holds.
    fn server_input(&self, served: Served) -> ServerInput<'_> {
        match served {
            Served::Bot { input } => {
                let input = self.input(input);
                ServerInput::Bot {
                    slot: input.slot,
                    payload: input.payload,
                }
            }
            Served::Join { slot, delegation } => ServerInput::Join {
                slot,
                delegation: self.delegations[delegation as usize].clone(),
            },
            Served::Renew { slot, delegation } => ServerInput::Renew {
                slot,
                delegation: self.delegations[delegation as usize].clone(),
            },
            Served::Leave {
                slot,
                reason,
                becomes,
            } => ServerInput::Leave {
                slot,
                reason,
                becomes,
            },
            Served::Connected { slot } => ServerInput::Connected { slot },
            Served::Disconnected { slot } => ServerInput::Disconnected { slot },
        }
    }

    /// Keeps `delegation`, a join's or a renewal's, and gives its index.
    fn push_delegation(&mut self, delegation: Delegation) -> u32 {
        let index = offset(self.delegations.len());
        self.delegations.push(delegation);
        index
    }

    /// Logs `input`'s payload, and gives its index.
    fn push_input(&mut self, input: PlayerInput<'_>) -> u32 {
        let index = offset(self.inputs.len());
        let payload = self.payloads.push(input.payload);
        self.inputs.push(LoggedInput {
            slot: input.slot,
            stamp: input.stamp,
            payload,
        });
        index
    }

    /// Queues again what the logged entry `at` scheduled, as a rewound log replays it; whether it
    /// is a server input.
    fn replay_entry(&mut self, at: u32) -> bool {
        match self.entries[at as usize] {
            Entry::Packet(packet) => {
                for index in self.packets[packet as usize].inputs.clone() {
                    self.schedule(index);
                }
                false
            }
            Entry::Server(server) => {
                match self.server[server as usize].served {
                    Served::Bot { input } => {
                        self.schedule(input);
                    }
                    Served::Join { slot, .. } | Served::Leave { slot, .. } => {
                        self.forget_pending(slot);
                    }
                    Served::Renew { .. }
                    | Served::Connected { .. }
                    | Served::Disconnected { .. } => {}
                }
                true
            }
        }
    }

    /// Drops the inputs of `slot` still to apply, and starts its spill afresh: its controller
    /// changes.
    fn forget_pending(&mut self, slot: PlayerSlot) {
        self.pending.retain(|Reverse(due)| due.slot != slot);
        self.slots[slot.index()].spill = Spill::default();
    }

    /// Queues the logged input `index`, logged before the next tick, for the tick it applies in,
    /// and says when that is: what `applied` says, or later, past the ticks its slot's inputs
    /// before it fill.
    fn schedule(&mut self, index: u32) -> Applied {
        let logged = &self.inputs[index as usize];
        let (slot, outcome) = (logged.slot, self.applied(logged.stamp));
        let Applied::At(earliest) = outcome else {
            return outcome;
        };
        let max = self.header.terms.inputs.max_per_tick;
        let tick = self.slots[slot.index()].spill.take(earliest, max);
        self.pending.push(Reverse(Due { tick, slot, index }));
        Applied::At(tick)
    }

    /// When an input stamped `stamp`, logged now, applies.
    fn applied(&self, stamp: Tick) -> Applied {
        let next = self.next_tick();
        if next
            .since(stamp)
            .is_some_and(|delay| delay > self.header.terms.inputs.max_delay)
        {
            return Applied::Late;
        }
        if stamp
            .since(next)
            .is_some_and(|lead| lead > self.header.terms.inputs.max_lead)
        {
            return Applied::Early;
        }
        Applied::At(stamp.max(next))
    }

    fn input(&self, index: u32) -> PlayerInput<'_> {
        let logged = &self.inputs[index as usize];
        PlayerInput {
            slot: logged.slot,
            stamp: logged.stamp,
            payload: self.payloads.get(logged.payload),
        }
    }
}

/// A position in the log's buffers, which the file's bound keeps within a `u32`.
fn offset(len: usize) -> u32 {
    u32::try_from(len).expect("log positions fit u32")
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
    use crate::session_log::SessionLog;

    impl SessionLog {
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
