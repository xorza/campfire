use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::mem;
use std::ops::Range;

use blake3::Hasher;
use campfire_common::{PlayerSlot, SegmentSeed, Tick, Ticks};
use secp256k1::{Secp256k1, VerifyOnly, XOnlyPublicKey};
use serde::{Deserialize, Serialize};

use crate::delegation::Delegation;
use crate::input_chain::InputChain;
use crate::player_input::PlayerInput;
use crate::server_input::error::ServerInputDecodeError;
use crate::server_input::{AfterLeave, InputPlace, ServerInput};
use crate::server_seed::ServerSeed;
use crate::session_id::SessionId;
use crate::session_log::error::{HeaderError, InputError, LogError, SeedError, ServerInputError};
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
/// for a slot the player controls. The server records entries as they arrive; decoding a
/// published log records them again, which checks every chain link and signature, and a verifier
/// then replays the decoded log's ticks with `rewound`.
#[derive(Debug)]
pub struct SessionLog {
    header: SessionHeader,
    /// Present once the segment is published.
    revealed: Option<ServerSeed>,
    secp: Secp256k1<VerifyOnly>,
    /// Each slot's controller and counts, by slot.
    slots: Vec<Slot>,
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
}

/// Who controls a slot.
#[derive(Debug, Clone, Copy)]
enum Control {
    /// A player, by their session key and main key, with their chain as logged so far.
    Player {
        session_key: XOnlyPublicKey,
        main_key: [u8; 32],
        chain: InputChain,
    },
    Bot,
    Open,
    /// None: it waits for the player who left it.
    Reserved,
}

/// A player's inputs as their stamps count them: the last stamp, the inputs of that stamp, and
/// the inputs in all.
#[derive(Debug, Clone, Copy, Default)]
struct StampCount {
    last: Option<Tick>,
    at_last: u32,
    total: u64,
}

/// The last tick a slot's inputs were scheduled to apply in, and how many apply there.
#[derive(Debug, Clone, Copy, Default)]
struct Spill {
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

/// A server input as logged, with its signature, and for a bot's commands the input they are.
#[derive(Debug)]
struct LoggedServer {
    input: ServerInput,
    signature: Signature,
    bot_input: Option<u32>,
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

impl Slot {
    /// A slot as its header start leaves it.
    fn of(slot: PlayerSlot, start: &SlotStart) -> Slot {
        let control = match start {
            SlotStart::Player(delegation) => Control::player(slot, delegation),
            SlotStart::Bot => Control::Bot,
            SlotStart::Open => Control::Open,
        };
        Slot {
            control,
            leaver: None,
            stamps: StampCount::default(),
            spill: Spill::default(),
        }
    }
}

impl Control {
    /// The player of `delegation` in `slot`, whose chain starts from its id.
    const fn player(slot: PlayerSlot, delegation: &Delegation) -> Control {
        Control::Player {
            session_key: delegation.terms().session_key,
            main_key: *delegation.main_key(),
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
        let mut slots = Vec::with_capacity(header.slots.len());
        for ((slot, start), &plan) in (0..).zip(&header.slots).zip(&header.terms.slots) {
            let slot = PlayerSlot::new(slot);
            if start.plan() != plan {
                return Err(HeaderError::PlanMismatch { slot });
            }
            if let SlotStart::Player(delegation) = start {
                delegation
                    .check(&header.terms.server_key, &session_id)
                    .map_err(|error| HeaderError::Scope { slot, error })?;
            }
            slots.push(Slot::of(slot, start));
        }
        Ok(SessionLog {
            slots,
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
        })
    }

    pub const fn header(&self) -> &SessionHeader {
        &self.header
    }

    /// The hash of the header's terms.
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Adds the first segment's server seed, which publishes the segment.
    pub fn reveal_seed(&mut self, server_seed: ServerSeed) {
        assert!(
            server_seed.check(0, &self.header.terms.seed_commitment),
            "the server reveals the seed it committed to"
        );
        self.revealed = Some(server_seed);
    }

    /// The first segment's server seed, once the segment is published.
    pub const fn revealed_seed(&self) -> Option<ServerSeed> {
        self.revealed
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
        let terms = &self.header.terms;
        let state = *self.slots.get(player).ok_or(InputError::UnknownPlayer)?;
        let Control::Player {
            session_key,
            main_key,
            mut chain,
        } = state.control
        else {
            return Err(InputError::NotPlayer);
        };
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
        held.control = Control::Player {
            session_key,
            main_key,
            chain,
        };
        held.stamps = stamps;
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
        input: &ServerInput,
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
            Control::Player { main_key, .. } => Some(main_key),
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
        input: ServerInput,
        signature: &Signature,
    ) -> Result<(), ServerInputError> {
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays takes no input"
        );
        let change = self.change_of(&input)?;
        let place = self.next_place();
        let server_key = &self.header.terms.server_key;
        if !input.signed_by(&self.secp, server_key, self.session_id, place, signature) {
            return Err(ServerInputError::BadSignature);
        }
        let slot = input.slot();
        let at = slot.index();
        let next = self.next_tick();
        let mut bot_input = None;
        match &input {
            ServerInput::Bot { payload, .. } => {
                let index = self.push_input(PlayerInput {
                    slot,
                    stamp: next,
                    payload,
                });
                self.schedule(index);
                bot_input = Some(index);
            }
            ServerInput::Join { delegation, .. } => {
                let held = &mut self.slots[at];
                held.control = Control::player(slot, delegation);
                held.leaver = None;
                held.stamps = StampCount::default();
                self.forget_pending(slot);
            }
            ServerInput::Renew { delegation, .. } => {
                if let Control::Player { session_key, .. } = &mut self.slots[at].control {
                    *session_key = delegation.terms().session_key;
                }
            }
            ServerInput::Leave { becomes, .. } => {
                let held = &mut self.slots[at];
                if let Control::Player { main_key, .. } = held.control {
                    held.leaver = Some(main_key);
                }
                held.control = match becomes {
                    AfterLeave::Reserve => Control::Reserved,
                    AfterLeave::Bot => Control::Bot,
                    AfterLeave::Open => Control::Open,
                };
                held.stamps = StampCount::default();
                self.forget_pending(slot);
            }
            ServerInput::Connected { .. } | ServerInput::Disconnected { .. } => {}
        }
        if let Some(kind) = change {
            self.changes.push(SlotChange {
                tick: next,
                slot,
                kind,
            });
        }
        self.entries.push(Entry::Server(offset(self.server.len())));
        self.server.push(LoggedServer {
            input,
            signature: *signature,
            bot_input,
        });
        self.server_since += 1;
        Ok(())
    }

    /// Closes the next tick to new entries and gives the inputs applied in it, by slot, then in
    /// the order logged; `sealed_changes` then gives the changes of a slot's controller in it.
    pub fn seal_tick(&mut self) -> impl ExactSizeIterator<Item = PlayerInput<'_>> {
        let tick = self.next_tick();
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
        self.due.iter().map(|&index| self.input(index))
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
    /// header, the `u64` number of sealed ticks, the entries logged before each tick, the entries
    /// logged since the last tick, and the revealed seed as an option. Entries go as a `u32`
    /// count, then each as a byte, 0 for a player's packet and 1 for a server input. A packet
    /// goes as its `u32` slot, its inputs as a `u32` count and each input's stamp and payload
    /// bytes, and its signature; a server input as its postcard encoding and its signature. An
    /// input's seq and link are not written: a reader computes them from the chain, and the
    /// signature covers them. The header goes field by field, each slot's start as a byte, 0 for
    /// a player, then the delegation's JSON, 1 for a bot and 2 for an open slot.
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.clear();
        out.extend_from_slice(LOG_TAG);
        self.put_header(out);
        put(out, &self.next_tick());
        let mut start = 0;
        for &end in &self.tick_ends {
            self.put_entries(out, start..end);
            start = end;
        }
        self.put_entries(out, start..offset(self.entries.len()));
        put(out, &self.revealed);
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
        let ticks: u64 = take(&mut rest)?;
        for _ in 0..ticks {
            log.record_logged(&mut rest, &mut inputs, &mut applied)?;
            drop(log.seal_tick());
        }
        log.record_logged(&mut rest, &mut inputs, &mut applied)?;
        if let Some(server_seed) = take::<Option<ServerSeed>>(&mut rest)? {
            if !server_seed.check(0, &log.header.terms.seed_commitment) {
                return Err(LogError::WrongSeed);
            }
            log.revealed = Some(server_seed);
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
                    let packet = &self.packets[at as usize];
                    let slot = self.inputs[packet.inputs.start as usize].slot;
                    put(out, &PACKET_ENTRY);
                    put(out, &slot.get());
                    put(out, &(packet.inputs.end - packet.inputs.start));
                    for index in packet.inputs.clone() {
                        let input = self.input(index);
                        put(out, &input.stamp);
                        put(out, input.payload);
                    }
                    put(out, &packet.signature);
                }
                Entry::Server(at) => {
                    let logged = &self.server[at as usize];
                    put(out, &SERVER_ENTRY);
                    logged.input.encode(out);
                    put(out, &logged.signature);
                }
            }
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
            let tick = self.next_tick();
            match take::<u8>(rest)? {
                PACKET_ENTRY => {
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
                        .map_err(|error| LogError::Input { tick, error })?;
                }
                SERVER_ENTRY => {
                    let (input, after) = ServerInput::take(rest).map_err(|error| match error {
                        ServerInputDecodeError::Malformed(
                            postcard::Error::DeserializeUnexpectedEnd,
                        ) => LogError::Truncated,
                        error => LogError::ServerDecode { tick, error },
                    })?;
                    *rest = after;
                    let signature = take(rest)?;
                    self.record_server(input, &signature)
                        .map_err(|error| LogError::Server { tick, error })?;
                }
                _ => return Err(LogError::UnknownEntry),
            }
        }
        Ok(())
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
                let logged = &self.server[server as usize];
                let slot = logged.input.slot();
                match (&logged.input, logged.bot_input) {
                    (_, Some(index)) => {
                        self.schedule(index);
                    }
                    (ServerInput::Join { .. } | ServerInput::Leave { .. }, None) => {
                        self.forget_pending(slot);
                    }
                    _ => {}
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
            _ => return Err(LogError::UnknownEntry),
        };
        slots.push(start);
    }
    Ok(SessionHeader { terms, slots })
}

#[cfg(test)]
mod tests;
