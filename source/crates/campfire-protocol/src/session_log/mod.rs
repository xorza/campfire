use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
use std::mem;
use std::ops::Range;

use blake3::Hasher;
use campfire_common::{PlayerSlot, SegmentSeed, Tick, Ticks};
use secp256k1::{Secp256k1, VerifyOnly};
use serde::{Deserialize, Serialize};

use crate::checkpoint::Checkpoint;
use crate::checkpoint::checkpoint_begun::CheckpointBegun;
use crate::checkpoint::error::CheckpointDecodeError;
use crate::checkpoint::log_carry::{CarriedControl, CarriedInput, CarriedSlot, LogCarry};
use crate::controller::Controller;
use crate::decoded::Decoded;
use crate::delegation::Delegation;
use crate::input_chain::InputChain;
use crate::input_hash::InputHash;
use crate::journal::Journal;
use crate::journal::error::JournalReplayError;
use crate::journal::record_sink::RecordSink;
use crate::player_input::PlayerInput;
use crate::server_input::error::ServerInputDecodeError;
use crate::server_input::{AfterLeave, InputPlace, LeaveReason, ServerInput};
use crate::server_seed::ServerSeed;
use crate::server_seeds::ServerSeeds;
use crate::session_id::SessionId;
use crate::session_log::error::{
    CheckpointError, HeaderError, InputError, LoadError, LogError, ResultError, SeedError,
    ServerInputError,
};
use crate::session_result::SessionResult;
use crate::session_terms::SessionTerms;
use crate::signature::Signature;
use crate::slot_change::{SlotChange, SlotChangeKind, Taken};
use crate::slot_start::SlotStart;

pub(crate) mod error;

/// Starts the segment seed, so no other BLAKE3 use can produce one.
const SEGMENT_SEED_DOMAIN: &[u8] = b"campfire/segment-seed/v1";
/// Starts every log file and states its protocol version, so other bytes are refused at once.
const LOG_TAG: &[u8] = b"campfire/session-log/v2";
/// The log's positions are `u32`: input indices, payload offsets and the count of entries each
/// tick ends at.
const POSITION_BOUND: usize = u32::MAX as usize;
/// The bytes that start a player's packet and a server input in the file.
const PACKET_ENTRY: u8 = 0;
const SERVER_ENTRY: u8 = 1;
/// The bytes that start each kind of journal record.
const JOURNAL_HEADER: u8 = 0;
const JOURNAL_PACKET: u8 = 1;
const JOURNAL_SERVER: u8 = 2;
const JOURNAL_SEALED: u8 = 3;
const JOURNAL_CHECKPOINT_DONE: u8 = 4;
const JOURNAL_RESULT: u8 = 5;
const JOURNAL_CHECKPOINT_BEGUN: u8 = 6;
const JOURNAL_LOADED: u8 = 7;

/// What the log fixes before the first tick: the session's terms, and how each of its slots
/// starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHeader {
    pub terms: SessionTerms,
    /// Each slot's start, by slot: a player's delegation, which lets their session key sign
    /// their chain heads, whose id their first input links to, and which carries their seed
    /// contribution; a bot; or open.
    pub slots: Vec<SlotStart>,
}

impl SessionHeader {
    /// Segment `segment`'s seed, `BLAKE3(domain ‖ u32 segment ‖ server seed ‖ contributions of
    /// the players who started, in slot order)`; an error when `server_seed` is not that
    /// segment's seed of the committed chain.
    pub fn segment_seed(
        &self,
        segment: u32,
        server_seed: &ServerSeed,
    ) -> Result<SegmentSeed, SeedError> {
        if !server_seed.check(segment, &self.terms.seed_commitment) {
            return Err(SeedError::WrongSeed);
        }
        let mut hasher = Hasher::new();
        hasher
            .update(SEGMENT_SEED_DOMAIN)
            .update(&segment.to_le_bytes())
            .update(server_seed.as_bytes());
        for (_, delegation) in self.players() {
            hasher.update(&delegation.terms().seed_contribution);
        }
        Ok(SegmentSeed::new(*hasher.finalize().as_bytes()))
    }

    /// The players who started, by slot, with their delegations.
    pub fn players(&self) -> impl Iterator<Item = (PlayerSlot, &Delegation)> {
        (0..)
            .zip(&self.slots)
            .filter_map(|(slot, start)| match start {
                SlotStart::Player(delegation) => Some((PlayerSlot::new(slot), &**delegation)),
                SlotStart::Bot | SlotStart::Open => None,
            })
    }
}

/// When a logged input takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    At(Tick),
    /// It arrived more than the max input delay after its stamp. It stays in the chain but never
    /// takes effect.
    Late,
    /// Its stamp was more than the max input lead ahead of the next tick. Like a late input, it
    /// stays in the chain but never takes effect.
    Early,
}

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
    secp: Secp256k1<VerifyOnly>,
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
    inputs: Vec<LoggedInput>,
    payloads: Vec<u8>,
    packets: Vec<Packet>,
    server: Vec<LoggedServer>,
    /// The packets and the server inputs, in the order they were logged.
    entries: Vec<Entry>,
    /// The server inputs logged since the last tick was sealed.
    server_since: u32,
    /// Every change of a slot's controller, in the order logged, so by tick.
    changes: Vec<SlotChange>,
    /// The changes of the tick last sealed.
    sealed_changes: Range<usize>,
    /// For each sealed tick, the end of the entries logged before it ran.
    tick_ends: Vec<u32>,
    /// While a rewound log replays, the tick ends it had; empty otherwise.
    to_replay: Vec<u32>,
    /// Inputs applied in a tick not yet sealed, earliest first.
    pending: BinaryHeap<Reverse<Due>>,
    /// Indices of the inputs applied in the tick last sealed.
    due: Vec<u32>,
    /// The most inputs and payload bytes the log holds: `POSITION_BOUND`, or less in a test.
    position_bound: usize,
    /// The hash of the header's terms.
    session_id: SessionId,
    /// Where each record the log takes goes as it takes it, once the server keeps one.
    journal: Option<Journal>,
    /// The journaled packets not known durable yet, by record, and where each left its chain.
    journaled: VecDeque<Journaled>,
    /// Where each slot's chain stands in the records the journal synced, by slot.
    durable: Vec<Option<DurableHead>>,
}

/// Where a player's chain stands after an input of theirs whose journal record is durable: the
/// id of the delegation whose key signed the chain head then, the input's seq, and the head after
/// it, as a receipt names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableHead {
    pub delegation: [u8; 32],
    pub seq: u64,
    pub head: InputHash,
}

/// A journaled packet: its record's index, its slot, and where it left its chain.
#[derive(Debug, Clone, Copy)]
struct Journaled {
    record: u64,
    slot: PlayerSlot,
    head: DurableHead,
}

/// A slot as the log follows it: who controls it, the main key of the player who left it last,
/// and its counts. Its spill counts its controller's inputs alone: a change of controller starts
/// it afresh, so in a bot's slot it counts the bot's.
#[derive(Debug, Clone, Copy)]
struct Slot {
    control: Control,
    leaver: Option<[u8; 32]>,
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

/// A player's inputs as their stamps count them: the last stamp, the inputs of that stamp, and
/// the inputs in all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StampCount {
    last: Option<Tick>,
    at_last: u32,
    total: u64,
}

/// The last tick a slot's inputs were scheduled to apply in, and how many apply there.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Spill {
    tick: Tick,
    count: u32,
}

#[derive(Debug)]
struct LoggedInput {
    slot: PlayerSlot,
    stamp: Tick,
    payload: Range<u32>,
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

impl StampCount {
    /// Counts one more input, stamped `stamp`; an error for a stamp before the last, or one
    /// more than `max` inputs of one stamp.
    const fn add(&mut self, stamp: Tick, max: u32) -> Result<(), InputError> {
        match self.last {
            Some(last) if stamp.get() < last.get() => return Err(InputError::StampBack),
            Some(last) if stamp.get() == last.get() => {
                if self.at_last == max {
                    return Err(InputError::TooManyInputs);
                }
                self.at_last += 1;
            }
            _ => {
                self.last = Some(stamp);
                self.at_last = 1;
            }
        }
        self.total += 1;
        Ok(())
    }
}

impl Spill {
    /// The tick an input that may apply from tick `earliest` on applies in: the first, from
    /// there and from the last, that fewer than `max` of the slot's inputs fill.
    const fn take(&mut self, earliest: Tick, max: u32) -> Tick {
        if self.count == 0 || earliest.get() > self.tick.get() {
            self.tick = earliest;
            self.count = 1;
        } else if self.count < max {
            self.count += 1;
        } else {
            self.tick = self.tick.after(Ticks::ONE);
            self.count = 1;
        }
        self.tick
    }

    /// How many of the slot's inputs apply in `tick` so far.
    const fn at(self, tick: Tick) -> u32 {
        if self.tick.get() == tick.get() {
            self.count
        } else {
            0
        }
    }
}

impl SegmentStart {
    /// Writes the begin of the checkpoint that starts segment `segment`, and its record once it
    /// has one, into `journal`.
    fn journal_whole(&self, journal: &mut Journal, segment: u32) {
        self.journal_begun(journal, segment);
        if let Some(logged) = &self.checkpoint {
            logged.journal(journal);
        }
    }

    /// Writes the begin of the checkpoint that starts segment `segment` into `journal`.
    fn journal_begun(&self, journal: &mut Journal, segment: u32) {
        journal.append(|out| {
            out.push(JOURNAL_CHECKPOINT_BEGUN);
            put(out, &segment);
            put(out, &self.tick);
        });
    }
}

impl LoggedCheckpoint {
    /// Writes the record into `journal`, as the checkpoint is done.
    fn journal(&self, journal: &mut Journal) {
        journal.append(|out| {
            out.push(JOURNAL_CHECKPOINT_DONE);
            self.record.encode(out);
            put(out, &self.signature);
        });
    }
}

impl Slot {
    /// A slot of `control` with no input yet.
    fn of(control: Control) -> Slot {
        Slot {
            control,
            leaver: None,
            stamps: StampCount::default(),
            spill: Spill::default(),
            head_signer: None,
        }
    }
}

impl Control {
    /// The player of `delegation`, the log's `index`th, in `slot`, whose chain starts from its
    /// id.
    const fn player(slot: PlayerSlot, delegation: &Delegation, index: u32) -> Control {
        Control::Player {
            delegation: index,
            chain: InputChain::new(slot, delegation.chain_root()),
        }
    }
}

impl SessionLog {
    /// A log with nothing recorded; an error when the header starts another count of slots than
    /// the terms plan, or a slot otherwise than its plan, when a player's delegation names
    /// another server or session than `header`, whose terms the session id hashes, or when there
    /// are more slots than a `u32` counts.
    pub fn new(header: SessionHeader) -> Result<SessionLog, HeaderError> {
        if u32::try_from(header.slots.len()).is_err() {
            return Err(HeaderError::TooManySlots);
        }
        if header.slots.len() != header.terms.slots.len() {
            return Err(HeaderError::SlotCount);
        }
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
                    let index = offset(delegations.len());
                    delegations.push((**delegation).clone());
                    Control::player(slot, delegation, index)
                }
                SlotStart::Bot => Control::Bot,
                SlotStart::Open => Control::Open,
            };
            slots.push(Slot::of(control));
        }
        Ok(SessionLog {
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
            secp: Secp256k1::verification_only(),
            inputs: Vec::new(),
            payloads: Vec::new(),
            packets: Vec::new(),
            server: Vec::new(),
            entries: Vec::new(),
            server_since: 0,
            changes: Vec::new(),
            sealed_changes: 0..0,
            tick_ends: Vec::new(),
            to_replay: Vec::new(),
            pending: BinaryHeap::new(),
            due: Vec::new(),
            position_bound: POSITION_BOUND,
            session_id,
            journal: None,
            journaled: VecDeque::new(),
            durable: vec![None; slots_len],
        })
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
        self.revealed = Some(server_seed);
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
    pub fn leaver(&self, slot: PlayerSlot) -> Option<[u8; 32]> {
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

    /// Logs a player's packet before the next tick: `inputs`, all of one slot, in the order sent,
    /// and the player's session-key `signature` over the chain head after the last. Writes when
    /// each input applies into `applied`, which is cleared first: late when the next tick is more
    /// than the max input delay after its stamp; early when its stamp is more than the max input
    /// lead ahead; else at `max(stamp, next tick)`, or later, when the player's inputs before it
    /// fill that tick: a player's inputs fill each tick up to the max inputs per tick, in chain
    /// order, and the rest spill to the next. A packet is refused for what its client controls
    /// alone, so one that follows the rules is never refused, however the network groups its
    /// packets: more inputs than the max inputs per tick, in the packet or of one stamp; a stamp
    /// before the one before it; more inputs in all than the ticks up to the max input lead
    /// hold; a payload past the max length; or a bad signature; and for a slot its player does
    /// not control now. Each count and length is checked before any input is hashed. A refused
    /// packet leaves the log unchanged.
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
        let inputs = inputs.into_iter();
        let slot = inputs.clone().next().ok_or(InputError::EmptyPacket)?.slot;
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
        let mut bytes = 0;
        for input in inputs.clone() {
            debug_assert_eq!(input.slot, slot, "a packet holds one player's inputs");
            if input.payload.len() > terms.max_payload_len as usize {
                return Err(InputError::PayloadTooLarge);
            }
            count += 1;
            if count > terms.max_inputs_per_tick {
                return Err(InputError::TooManyInputs);
            }
            stamps.add(input.stamp, terms.max_inputs_per_tick)?;
            bytes += input.payload.len();
        }
        let ticks = self
            .next_tick()
            .get()
            .saturating_add(terms.max_input_lead.get())
            .saturating_add(1);
        if stamps.total > ticks.saturating_mul(u64::from(terms.max_inputs_per_tick)) {
            return Err(InputError::AheadOfTime);
        }
        let count = count as usize;
        if self.inputs.len() + count > self.position_bound
            || self.payloads.len() + bytes > self.position_bound
            || self.entries.len() >= self.position_bound
        {
            return Err(InputError::LogFull);
        }
        for input in inputs.clone() {
            chain.extend(input.stamp, input.payload);
        }
        if !chain.signed_by(&self.secp, &session_key, self.session_id, signature) {
            return Err(InputError::BadSignature);
        }

        let held = &mut self.slots[player];
        held.control = Control::Player { delegation, chain };
        held.stamps = stamps;
        held.head_signer = Some(delegation);
        applied.clear();
        applied.reserve_exact(count);
        let start = offset(self.inputs.len());
        for input in inputs {
            let index = self.push_input(input);
            applied.push(self.schedule(index));
        }
        self.entries.push(Entry::Packet(offset(self.packets.len())));
        self.packets.push(Packet {
            inputs: start..offset(self.inputs.len()),
            signature: *signature,
        });
        self.journal_last_entry();
        if let Some(journal) = &self.journal {
            self.journaled.push_back(Journaled {
                record: journal.written() - 1,
                slot,
                head: DurableHead {
                    delegation: *self.delegations[delegation as usize].id(),
                    seq: chain.next_seq() - 1,
                    head: chain.head(),
                },
            });
        }
        Ok(())
    }

    /// What a server input would do to its slot's controller, logged now, as its structure
    /// allows, before its signature is checked: a join from what the slot is, a leave to what it
    /// becomes, or nothing; an error for an input the log refuses. A bot's commands need a slot a
    /// bot plays, a payload within the max length and a place within the tick's max inputs; a
    /// join a slot no player controls, and of a reserved one its leaver; a renewal, a leave and a
    /// link's change a slot a player controls, a renewal by the same main key; a delegation
    /// this session's.
    pub fn change_of(
        &self,
        input: &ServerInput<'_>,
    ) -> Result<Option<SlotChangeKind>, ServerInputError> {
        let terms = &self.header.terms;
        let state = self
            .slots
            .get(input.slot().index())
            .ok_or(ServerInputError::UnknownSlot)?;
        if self.entries.len() >= self.position_bound {
            return Err(ServerInputError::LogFull);
        }
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
                if payload.len() > terms.max_payload_len as usize {
                    return Err(ServerInputError::PayloadTooLarge);
                }
                if state.spill.at(self.next_tick()) >= terms.max_inputs_per_tick {
                    return Err(ServerInputError::TooManyInputs);
                }
                if self.inputs.len() >= self.position_bound
                    || self.payloads.len() + payload.len() > self.position_bound
                {
                    return Err(ServerInputError::LogFull);
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
    /// its place, the next server input logged there: what `change_of` allows, a bot's commands
    /// applying in the next tick in slot order, among the players' inputs. A join or a leave of
    /// a slot drops the inputs its player logged that were still to apply, and changes the
    /// controller from the next tick on. A refused input leaves the log unchanged.
    pub fn record_server(
        &mut self,
        input: ServerInput<'_>,
        signature: &Signature,
    ) -> Result<(), ServerInputError> {
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays takes no input"
        );
        debug_assert!(self.result.is_none(), "a session that ended takes no input");
        let change = self.change_of(&input)?;
        let place = self.next_place();
        let server_key = &self.header.terms.server_key;
        if !input.signed_by(&self.secp, server_key, self.session_id, place, signature) {
            return Err(ServerInputError::BadSignature);
        }
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
                let control = Control::player(slot, &delegation, offset(self.delegations.len()));
                let index = self.push_delegation(delegation);
                let held = &mut self.slots[at];
                held.control = control;
                held.leaver = None;
                held.stamps = StampCount::default();
                held.head_signer = None;
                self.forget_pending(slot);
                self.forget_durable(slot);
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
                self.forget_durable(slot);
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
                self.forget_durable(slot);
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
        self.journal_last_entry();
        Ok(())
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
    /// start that segment, at its tick, and carry the log's own state there, so a segment
    /// verifies from its checkpoint alone. A refused record leaves the log unchanged.
    pub fn record_checkpoint(
        &mut self,
        record: Checkpoint,
        signature: &Signature,
    ) -> Result<(), CheckpointError> {
        let begun = self.begun.as_ref().ok_or(CheckpointError::NotBegun)?;
        let server_key = &self.header.terms.server_key;
        if !record.signed_by(&self.secp, server_key, self.session_id, signature) {
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
        if !result.signed_by(&self.secp, server_key, self.session_id, signature) {
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
            journal.append(|out| {
                out.push(JOURNAL_RESULT);
                put(out, &logged);
            });
        }
        self.result = Some(logged);
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
        if self.to_replay.is_empty()
            && let Some(journal) = &mut self.journal
        {
            journal.append(|out| out.push(JOURNAL_SEALED));
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
            self.to_replay = Vec::new();
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
    pub fn rewound(mut self) -> SessionLog {
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

    /// Writes the log file into `out`, which is cleared first: the tag, then in postcard the
    /// header; the `u32` number of segments, each as its checkpoint record and the signature over
    /// it, but the first, which has none, then the `u64` number of its sealed ticks and the
    /// entries logged before each; the entries logged since the last tick; the revealed seed as
    /// an option; and the result with its signature as an option. Entries go as a `u32` count,
    /// then each as a byte, 0 for a player's packet and 1 for a server input. A packet goes as its
    /// `u32` slot, its inputs as a `u32` count and each input's stamp and payload bytes, and its
    /// signature; a server input as its postcard encoding and its signature. An input's seq and
    /// link are not written: a reader computes them from the chain, and the signature covers
    /// them. The header goes field by field, each slot's start as a byte, 0 for a player, then
    /// the delegation's JSON, 1 for a bot and 2 for an open slot.
    pub fn encode(&self, out: &mut Vec<u8>) {
        assert!(
            self.begun.is_none(),
            "a log is written once every checkpoint it began has its record"
        );
        let start = self.put_segments(out, self.segments.len(), self.next_tick());
        self.put_entries(out, start..offset(self.entries.len()));
        put(out, &self.revealed);
        put(out, &self.result);
    }

    /// Writes into `out`, which is cleared first, the file of the log as it stood at the boundary
    /// where segment `segment` starts: its segments up to that one, their ticks before it, and
    /// none of the entries logged after them.
    fn encode_through(&self, segment: usize, out: &mut Vec<u8>) {
        let tick = self.segments[segment].tick;
        let start = self.put_segments(out, segment + 1, tick);
        self.put_entries(out, start..start);
        put(out, &None::<ServerSeed>);
        put(out, &None::<LoggedResult>);
    }

    /// Writes into `out`, which is cleared first, the tag, the header, and the first `segments`
    /// segments, the last ending before `end`; where the entries of the ticks written end.
    fn put_segments(&self, out: &mut Vec<u8>, segments: usize, end: Tick) -> u32 {
        out.clear();
        out.extend_from_slice(LOG_TAG);
        self.put_header(out);
        put(out, &offset(segments));
        let mut start = 0;
        for (at, segment) in self.segments[..segments].iter().enumerate() {
            if let Some(logged) = &segment.checkpoint {
                logged.record.encode(out);
                put(out, &logged.signature);
            }
            let end = self
                .segments
                .get(at + 1)
                .filter(|_| at + 1 < segments)
                .map_or(end, |next| next.tick);
            put(out, &(end.get() - segment.tick.get()));
            for tick in segment.tick.get()..end.get() {
                let tick_end = self.tick_ends[usize::try_from(tick).expect("tick fits usize")];
                self.put_entries(out, start..tick_end);
                start = tick_end;
            }
        }
        start
    }

    /// Goes back to the boundary where segment `segment` starts, its checkpoint's, as a load of
    /// that save does: the log becomes what it held there, its segments after that one and every
    /// entry logged after the boundary dropped, and the segment goes on from its checkpoint, with
    /// its seed. The journal logs the load, so a log rebuilt from it is the log loaded. An error
    /// when the log holds no checkpoint that starts the segment.
    pub fn load(&mut self, segment: u32) -> Result<(), LoadError> {
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
            return Err(LoadError::NoCheckpoint);
        }
        let mut bytes = Vec::new();
        self.encode_through(at, &mut bytes);
        let mut loaded = SessionLog::decode_within(&bytes, self.position_bound)
            .expect("the log's own past decodes");
        if let Some(mut journal) = self.journal.take() {
            journal.append(|out| {
                out.push(JOURNAL_LOADED);
                put(out, &segment);
            });
            loaded.journal = Some(journal);
        }
        loaded.loaded = true;
        *self = loaded;
        Ok(())
    }

    /// Decodes a log file. Its entries are recorded again, which checks every chain link and
    /// signature, and only the bytes `encode` gives for the decoded log are accepted, so a log has
    /// one file.
    pub fn decode(bytes: &[u8]) -> Result<SessionLog, LogError> {
        SessionLog::decode_within(bytes, POSITION_BOUND)
    }

    fn decode_within(bytes: &[u8], position_bound: usize) -> Result<SessionLog, LogError> {
        let mut rest = bytes.strip_prefix(LOG_TAG).ok_or(LogError::NotLog)?;
        let header = take_header(&mut rest)?;
        let mut log = SessionLog::new(header).map_err(LogError::Header)?;
        log.position_bound = position_bound;
        let mut inputs = Vec::new();
        let mut applied = Vec::new();
        let segments: u32 = take(&mut rest)?;
        if segments == 0 {
            return Err(LogError::NoSegment);
        }
        for segment in 0..segments {
            if segment > 0 {
                log.record_checkpoint_at(&mut rest, segment)?;
            }
            let ticks: u64 = take(&mut rest)?;
            for _ in 0..ticks {
                log.record_logged(&mut rest, &mut inputs, &mut applied)?;
                drop(log.seal_tick());
            }
        }
        log.record_logged(&mut rest, &mut inputs, &mut applied)?;
        if let Some(server_seed) = take::<Option<ServerSeed>>(&mut rest)? {
            if !server_seed.check(log.segment(), &log.header.terms.seed_commitment) {
                return Err(LogError::WrongSeed);
            }
            log.revealed = Some(server_seed);
        }
        if let Some(LoggedResult { result, signature }) = take(&mut rest)? {
            log.record_result(result, &signature)
                .map_err(LogError::Result)?;
        }
        if !rest.is_empty() {
            return Err(LogError::Trailing);
        }
        let mut canonical = Vec::with_capacity(bytes.len());
        log.encode(&mut canonical);
        if canonical != bytes {
            return Err(LogError::NotCanonical);
        }
        Ok(log)
    }

    fn put_header(&self, out: &mut Vec<u8>) {
        put(out, &self.header.terms);
        put(out, &offset(self.header.slots.len()));
        for start in &self.header.slots {
            put(out, &start.plan().code());
            if let SlotStart::Player(delegation) = start {
                put(out, delegation.json());
            }
        }
    }

    /// Writes the entries at `entries` of the log's.
    fn put_entries(&self, out: &mut Vec<u8>, entries: Range<u32>) {
        put(out, &(entries.end - entries.start));
        for &entry in &self.entries[entries.start as usize..entries.end as usize] {
            match entry {
                Entry::Packet(at) => {
                    put(out, &PACKET_ENTRY);
                    self.put_packet(out, at);
                }
                Entry::Server(at) => {
                    put(out, &SERVER_ENTRY);
                    self.put_server(out, at);
                }
            }
        }
    }

    /// Writes the packet `at`: its `u32` slot, its inputs as a `u32` count and each input's stamp
    /// and payload bytes, and its signature.
    fn put_packet(&self, out: &mut Vec<u8>, at: u32) {
        let packet = &self.packets[at as usize];
        let slot = self.inputs[packet.inputs.start as usize].slot;
        put(out, &slot.get());
        put(out, &(packet.inputs.end - packet.inputs.start));
        for index in packet.inputs.clone() {
            let input = self.input(index);
            put(out, &input.stamp);
            put(out, input.payload);
        }
        put(out, &packet.signature);
    }

    /// Writes the server input `at` and its signature.
    fn put_server(&self, out: &mut Vec<u8>, at: u32) {
        let logged = &self.server[at as usize];
        self.server_input(logged.served).encode(out);
        put(out, &logged.signature);
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

    /// Records the entries `put_entries` wrote at the front of `rest` before the next tick, with
    /// `inputs` and `applied` as scratch.
    fn record_logged<'a>(
        &mut self,
        rest: &mut &'a [u8],
        inputs: &mut Vec<PlayerInput<'a>>,
        applied: &mut Vec<Applied>,
    ) -> Result<(), LogError> {
        let entries: u32 = take(rest)?;
        for _ in 0..entries {
            match take::<u8>(rest)? {
                PACKET_ENTRY => self.record_packet(rest, inputs, applied)?,
                SERVER_ENTRY => self.record_server_input(rest)?,
                _ => return Err(LogError::UnknownEntry),
            }
        }
        Ok(())
    }

    /// Records the packet `put_packet` wrote at the front of `rest`, with `inputs` and `applied`
    /// as scratch.
    fn record_packet<'a>(
        &mut self,
        rest: &mut &'a [u8],
        inputs: &mut Vec<PlayerInput<'a>>,
        applied: &mut Vec<Applied>,
    ) -> Result<(), LogError> {
        let tick = self.next_tick();
        let slot = PlayerSlot::new(take(rest)?);
        let count: u32 = take(rest)?;
        inputs.clear();
        for _ in 0..count {
            let stamp = take(rest)?;
            let payload = take(rest)?;
            inputs.push(PlayerInput {
                slot,
                stamp,
                payload,
            });
        }
        let signature = take(rest)?;
        self.record(inputs.iter().copied(), &signature, applied)
            .map_err(|error| LogError::Input { tick, error })
    }

    /// Records the server input `put_server` wrote at the front of `rest`.
    fn record_server_input(&mut self, rest: &mut &[u8]) -> Result<(), LogError> {
        let tick = self.next_tick();
        let Decoded {
            value: input,
            rest: after,
        } = ServerInput::take(rest).map_err(|error| match error {
            ServerInputDecodeError::Malformed(postcard::Error::DeserializeUnexpectedEnd) => {
                LogError::Truncated
            }
            error => LogError::ServerDecode { tick, error },
        })?;
        *rest = after;
        let signature = take(rest)?;
        self.record_server(input, &signature)
            .map_err(|error| LogError::Server { tick, error })
    }

    /// Begins the checkpoint of segment `segment`, and records its record that `encode` wrote at
    /// the front of `rest`, with its signature.
    fn record_checkpoint_at(&mut self, rest: &mut &[u8], segment: u32) -> Result<(), LogError> {
        self.begin_checkpoint()
            .map_err(|error| LogError::Checkpoint { segment, error })?;
        self.record_done(rest, segment)
    }

    /// Records the record of the checkpoint begun, of segment `segment`, that `encode` wrote at
    /// the front of `rest`, with its signature.
    fn record_done(&mut self, rest: &mut &[u8], segment: u32) -> Result<(), LogError> {
        let Decoded {
            value: record,
            rest: after,
        } = Checkpoint::take(rest).map_err(|error| match error {
            CheckpointDecodeError::Malformed(postcard::Error::DeserializeUnexpectedEnd) => {
                LogError::Truncated
            }
            error => LogError::CheckpointDecode { segment, error },
        })?;
        *rest = after;
        let signature = take(rest)?;
        self.record_checkpoint(record, &signature)
            .map_err(|error| LogError::Checkpoint { segment, error })
    }

    /// The log of the session whose journal holds `records`, in order, as the server took them:
    /// what the server had logged when it wrote the last. An error for a first record that is
    /// not the header, a record after the result, a checkpoint's begin after an entry of its
    /// tick, and a record that does not decode or that the log refuses.
    pub fn from_journal<'a>(
        records: impl IntoIterator<Item = &'a [u8]>,
    ) -> Result<SessionLog, JournalReplayError> {
        let failed = |record, error| JournalReplayError::Record { record, error };
        let mut records = records.into_iter();
        let first = records.next().ok_or(JournalReplayError::NoHeader)?;
        let mut rest = first
            .strip_prefix(&[JOURNAL_HEADER])
            .ok_or(JournalReplayError::NoHeader)?;
        let header = take_header(&mut rest).map_err(|error| failed(0, error))?;
        if !rest.is_empty() {
            return Err(failed(0, LogError::Trailing));
        }
        let mut log =
            SessionLog::new(header).map_err(|error| failed(0, LogError::Header(error)))?;
        let mut inputs = Vec::new();
        let mut applied = Vec::new();
        for (index, record) in (1..).zip(records) {
            if log.result.is_some() {
                return Err(JournalReplayError::AfterResult { record: index });
            }
            if record.first() == Some(&JOURNAL_CHECKPOINT_BEGUN) && !log.at_boundary() {
                return Err(JournalReplayError::CheckpointMidTick { record: index });
            }
            log.apply_journal(record, &mut inputs, &mut applied)
                .map_err(|error| failed(index, error))?;
        }
        Ok(log)
    }

    /// Whether no entry was logged since the last tick was sealed: where a checkpoint may come.
    fn at_boundary(&self) -> bool {
        self.entries.len() == self.tick_ends.last().map_or(0, |&end| end as usize)
    }

    /// Takes the journal record `record`, past the header, with `inputs` and `applied` as
    /// scratch.
    fn apply_journal<'a>(
        &mut self,
        record: &'a [u8],
        inputs: &mut Vec<PlayerInput<'a>>,
        applied: &mut Vec<Applied>,
    ) -> Result<(), LogError> {
        let (&kind, mut rest) = record.split_first().ok_or(LogError::Truncated)?;
        match kind {
            JOURNAL_PACKET => self.record_packet(&mut rest, inputs, applied)?,
            JOURNAL_SERVER => self.record_server_input(&mut rest)?,
            JOURNAL_SEALED => drop(self.seal_tick()),
            JOURNAL_CHECKPOINT_BEGUN => {
                let segment: u32 = take(&mut rest)?;
                let tick: Tick = take(&mut rest)?;
                let begun = self
                    .begin_checkpoint()
                    .map_err(|error| LogError::Checkpoint { segment, error })?;
                if (begun.segment, begun.tick) != (segment, tick) {
                    let error = if begun.segment == segment {
                        CheckpointError::Tick
                    } else {
                        CheckpointError::Segment
                    };
                    return Err(LogError::Checkpoint { segment, error });
                }
            }
            JOURNAL_CHECKPOINT_DONE => {
                let segment = self.segment();
                self.record_done(&mut rest, segment)?;
            }
            JOURNAL_LOADED => {
                let segment: u32 = take(&mut rest)?;
                self.load(segment)
                    .map_err(|error| LogError::Load { segment, error })?;
            }
            JOURNAL_RESULT => {
                let LoggedResult { result, signature } = take(&mut rest)?;
                self.record_result(result, &signature)
                    .map_err(LogError::Result)?;
            }
            _ => return Err(LogError::UnknownEntry),
        }
        if !rest.is_empty() {
            return Err(LogError::Trailing);
        }
        Ok(())
    }

    /// Keeps `sink` as its journal, a new one, and writes into it the records of what the log
    /// holds: the header, then each entry, each sealed tick, each checkpoint and the result, as
    /// the log took them; from then on each record the log takes goes to it as it takes it.
    pub fn keep_journal(&mut self, sink: Box<dyn RecordSink + Send + Sync>) {
        assert!(self.journal.is_none(), "a log keeps one journal");
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays keeps no journal"
        );
        let mut journal = Journal::new(sink);
        journal.append(|out| {
            out.push(JOURNAL_HEADER);
            self.put_header(out);
        });
        let mut start = 0;
        let mut segments = (1..).zip(self.segments.iter().skip(1)).peekable();
        for (tick, &end) in (0..).zip(&self.tick_ends) {
            if let Some((number, segment)) =
                segments.next_if(|(_, segment)| segment.tick == Tick::new(tick))
            {
                segment.journal_whole(&mut journal, number);
            }
            for at in start..end {
                self.journal_entry(&mut journal, at);
            }
            journal.append(|out| out.push(JOURNAL_SEALED));
            start = end;
        }
        if let Some((number, segment)) = segments.next() {
            segment.journal_whole(&mut journal, number);
        }
        for at in start..offset(self.entries.len()) {
            self.journal_entry(&mut journal, at);
        }
        if let Some(result) = &self.result {
            journal.append(|out| {
                out.push(JOURNAL_RESULT);
                put(out, result);
            });
        }
        self.journal = Some(journal);
    }

    /// Keeps `sink` as its journal, which holds every record the log took, as the log was
    /// rebuilt from it: from then on each record the log takes goes to it as it takes it. Every
    /// chain whose head its player's current delegation signed stands durably where the log
    /// holds it, as the journal was read back from the disk.
    pub fn resume_journal(&mut self, sink: Box<dyn RecordSink + Send + Sync>) {
        assert!(self.journal.is_none(), "a log keeps one journal");
        self.journal = Some(Journal::new(sink));
        for (durable, held) in self.durable.iter_mut().zip(&self.slots) {
            let Control::Player { delegation, chain } = held.control else {
                continue;
            };
            // A player who renewed their delegation since their last input has no head of theirs.
            if held.head_signer != Some(delegation) {
                continue;
            }
            *durable = chain.next_seq().checked_sub(1).map(|seq| DurableHead {
                delegation: *self.delegations[delegation as usize].id(),
                seq,
                head: chain.head(),
            });
        }
    }

    /// Notes where each player's chain stands once the first `durable` records the log wrote
    /// into its journal's sink are synced, as the sink's writer counts them.
    pub fn advance_durable(&mut self, durable: u64) {
        debug_assert!(
            self.journal
                .as_ref()
                .is_some_and(|journal| durable <= journal.written()),
            "durable records are records the log wrote into its journal"
        );
        while let Some(journaled) = self.journaled.front()
            && journaled.record < durable
        {
            self.durable[journaled.slot.index()] = Some(journaled.head);
            self.journaled.pop_front();
        }
    }

    /// Where `slot`'s player's chain stands in the records the journal synced, as of the last
    /// `advance_durable`; none before any, and since a player took the slot, renewed their
    /// delegation, or left it.
    pub fn durable_head(&self, slot: PlayerSlot) -> Option<DurableHead> {
        self.durable.get(slot.index()).copied().flatten()
    }

    /// Writes the entry the log took last into its journal, when it keeps one.
    fn journal_last_entry(&mut self) {
        if let Some(mut journal) = self.journal.take() {
            self.journal_entry(&mut journal, offset(self.entries.len() - 1));
            self.journal = Some(journal);
        }
    }

    /// Writes the entry `at` into `journal`.
    fn journal_entry(&self, journal: &mut Journal, at: u32) {
        journal.append(|out| match self.entries[at as usize] {
            Entry::Packet(packet) => {
                out.push(JOURNAL_PACKET);
                self.put_packet(out, packet);
            }
            Entry::Server(server) => {
                out.push(JOURNAL_SERVER);
                self.put_server(out, server);
            }
        });
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
        let payload_start = offset(self.payloads.len());
        self.payloads.extend_from_slice(input.payload);
        self.inputs.push(LoggedInput {
            slot: input.slot,
            stamp: input.stamp,
            payload: payload_start..offset(self.payloads.len()),
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

    /// Forgets where `slot`'s chain stands durably: a player took the slot, renewed their
    /// delegation, or left it.
    fn forget_durable(&mut self, slot: PlayerSlot) {
        self.durable[slot.index()] = None;
        self.journaled.retain(|journaled| journaled.slot != slot);
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
        let max = self.header.terms.max_inputs_per_tick;
        let tick = self.slots[slot.index()].spill.take(earliest, max);
        self.pending.push(Reverse(Due { tick, slot, index }));
        Applied::At(tick)
    }

    /// When an input stamped `stamp`, logged now, applies.
    fn applied(&self, stamp: Tick) -> Applied {
        let next = self.next_tick();
        if next
            .since(stamp)
            .is_some_and(|delay| delay > self.header.terms.max_input_delay)
        {
            return Applied::Late;
        }
        if stamp
            .since(next)
            .is_some_and(|lead| lead > self.header.terms.max_input_lead)
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
            payload: &self.payloads[logged.payload.start as usize..logged.payload.end as usize],
        }
    }
}

/// A position in the log's buffers, which `record` keeps within `POSITION_BOUND`.
fn offset(len: usize) -> u32 {
    u32::try_from(len).expect("log positions fit u32")
}

fn put<T: Serialize + ?Sized>(out: &mut Vec<u8>, value: &T) {
    postcard::to_io(value, out).expect("postcard into a Vec cannot fail");
}

/// Reads one value off the front of `rest`.
fn take<'a, T: Deserialize<'a>>(rest: &mut &'a [u8]) -> Result<T, LogError> {
    let (value, after) = postcard::take_from_bytes(rest).map_err(|error| match error {
        postcard::Error::DeserializeUnexpectedEnd => LogError::Truncated,
        error => LogError::Malformed(error),
    })?;
    *rest = after;
    Ok(value)
}

/// Reads the header `SessionLog::put_header` wrote, checking each delegation.
fn take_header(rest: &mut &[u8]) -> Result<SessionHeader, LogError> {
    let terms: SessionTerms = take(rest)?;
    let count: u32 = take(rest)?;
    let mut slots = Vec::new();
    for slot in 0..count {
        let start = match take::<u8>(rest)? {
            0 => {
                let json = take(rest)?;
                let delegation = Delegation::parse(json).map_err(|error| {
                    LogError::Header(HeaderError::Delegation {
                        slot: PlayerSlot::new(slot),
                        error,
                    })
                })?;
                SlotStart::player(delegation)
            }
            1 => SlotStart::Bot,
            2 => SlotStart::Open,
            _ => {
                return Err(LogError::Header(HeaderError::UnknownStart {
                    slot: PlayerSlot::new(slot),
                }));
            }
        };
        slots.push(start);
    }
    Ok(SessionHeader { terms, slots })
}

#[cfg(test)]
mod tests;
