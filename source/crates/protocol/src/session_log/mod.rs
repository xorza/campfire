use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::mem;
use std::ops::Range;

use blake3::Hasher;
use campfire_math::SegmentSeed;
use secp256k1::{Secp256k1, VerifyOnly};
use serde::{Deserialize, Serialize};

use crate::chain_signature::ChainSignature;
use crate::delegation::Delegation;
use crate::delegation::error::DelegationError;
use crate::input_chain::InputChain;
use crate::player_input::PlayerInput;
use crate::player_slot::PlayerSlot;
use crate::server_seed::ServerSeed;
use crate::session_id::SessionId;
use crate::session_log::error::{HeaderError, InputError, LogError, SeedError};
use crate::session_terms::SessionTerms;

pub(crate) mod error;

/// Starts the segment seed, so no other BLAKE3 use can produce one.
const SEGMENT_SEED_DOMAIN: &[u8] = b"campfire/segment-seed/v1";
/// Starts every log file and states its protocol version, so other bytes are refused at once.
const LOG_TAG: &[u8] = b"campfire/session-log/v1";
/// The log's positions are `u32`: input indices, payload offsets and the count of inputs each
/// tick ends at.
const POSITION_BOUND: usize = u32::MAX as usize;

/// What the log fixes before the first tick: the session's terms, and its players.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHeader {
    pub terms: SessionTerms,
    /// Each player's delegation, by slot. It lets the player's session key sign their chain
    /// heads, its id is what the player's first input links to, and it carries the player's seed
    /// contribution.
    pub players: Vec<Delegation>,
}

impl SessionHeader {
    /// Segment `segment`'s seed, `BLAKE3(domain ‖ u32 segment ‖ server seed ‖ contributions in
    /// slot order)`; an error when `server_seed` is not that segment's seed of the committed chain.
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
        for delegation in &self.players {
            hasher.update(&delegation.terms().seed_contribution);
        }
        Ok(SegmentSeed::new(*hasher.finalize().as_bytes()))
    }
}

/// When a logged input takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    At(u64),
    /// It arrived more than the max input delay after its stamp. It stays in the chain but never
    /// takes effect.
    Late,
    /// Its stamp was more than the max input lead ahead of the next tick. Like a late input, it
    /// stays in the chain but never takes effect.
    Early,
}

/// The inputs of a session, in memory, in the order they were logged, and grouped by the tick
/// that was next when each arrived. The applied tick follows from that group, so a log cannot
/// hold a wrong one. Inputs arrive in packets, each signed once over the player's chain head
/// after it, so every logged input is signed. The server records packets as they arrive; decoding
/// a published log records them again, which checks every chain link and signature, and a
/// verifier then replays the decoded log's ticks with `rewound`.
#[derive(Debug)]
pub struct SessionLog {
    header: SessionHeader,
    /// Present once the segment is published.
    revealed: Option<ServerSeed>,
    secp: Secp256k1<VerifyOnly>,
    /// Each player's chain as logged so far, by slot.
    chains: Vec<InputChain>,
    /// Each player's inputs logged since the last sealed tick, by slot.
    sent_this_tick: Vec<u32>,
    inputs: Vec<LoggedInput>,
    payloads: Vec<u8>,
    /// The packets in the order they were logged; each ends where the next starts.
    packets: Vec<PacketEnd>,
    /// For each sealed tick, the end of the inputs logged before it ran.
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

#[derive(Debug)]
struct LoggedInput {
    slot: PlayerSlot,
    stamp: u64,
    payload: Range<u32>,
}

#[derive(Debug)]
struct PacketEnd {
    /// The end of the packet's inputs.
    end: u32,
    signature: ChainSignature,
}

/// Orders the inputs of one tick by slot, then by when they were logged, which for one player is
/// their chain's order, whatever order the players' inputs arrived in, so the host cannot choose
/// who acts first.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Due {
    tick: u64,
    slot: PlayerSlot,
    index: u32,
}

/// A packet as the log holds it: one player's inputs, and the signature over the chain head after
/// the last.
#[derive(Debug, Clone)]
struct Packet<'a> {
    log: &'a SessionLog,
    inputs: Range<u32>,
    signature: ChainSignature,
}

impl<'a> Packet<'a> {
    fn inputs(&self) -> impl ExactSizeIterator<Item = PlayerInput<'a>> + use<'a> {
        let log = self.log;
        self.inputs.clone().map(move |index| log.input(index))
    }
}

impl SessionLog {
    /// A log with nothing recorded; an error when a player's delegation names another server or
    /// session than `header`, whose terms the session id hashes, or there are more players than
    /// slots.
    pub fn new(header: SessionHeader) -> Result<SessionLog, HeaderError> {
        if u32::try_from(header.players.len()).is_err() {
            return Err(HeaderError::TooManyPlayers);
        }
        let session_id = header.terms.session_id();
        let mut chains = Vec::with_capacity(header.players.len());
        for (slot, delegation) in (0..).zip(&header.players) {
            let slot = PlayerSlot::new(slot);
            let terms = delegation.terms();
            let flaw = if terms.server_key != header.terms.server_key {
                Some(DelegationError::OtherServer)
            } else if terms.session_id != session_id {
                Some(DelegationError::OtherSession)
            } else {
                None
            };
            if let Some(error) = flaw {
                return Err(HeaderError::Delegation { slot, error });
            }
            chains.push(InputChain::new(slot, delegation.chain_root()));
        }
        Ok(SessionLog {
            sent_this_tick: vec![0; header.players.len()],
            header,
            revealed: None,
            secp: Secp256k1::verification_only(),
            chains,
            inputs: Vec::new(),
            payloads: Vec::new(),
            packets: Vec::new(),
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

    /// The tick that the inputs recorded now arrive before.
    pub fn next_tick(&self) -> u64 {
        u64::try_from(self.tick_ends.len()).expect("tick count fits u64")
    }

    /// Logs a player's packet before the next tick: `inputs`, all of one slot, in the order sent,
    /// and the player's session-key `signature` over the chain head after the last. Writes when
    /// each input applies into `applied`, which is cleared first: at `max(stamp, next tick)`;
    /// late when that is more than the max input delay after the stamp; early when the stamp is
    /// more than the max input lead ahead. A refused packet leaves the log unchanged.
    pub fn record<'a, I>(
        &mut self,
        inputs: I,
        signature: &ChainSignature,
        applied: &mut Vec<Applied>,
    ) -> Result<(), InputError>
    where
        I: IntoIterator<Item = PlayerInput<'a>>,
        I::IntoIter: Clone,
    {
        let inputs = inputs.into_iter();
        let slot = inputs.clone().next().ok_or(InputError::EmptyPacket)?.slot;
        let player = slot.get() as usize;
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays takes no input"
        );
        let mut chain = *self.chains.get(player).ok_or(InputError::UnknownPlayer)?;
        let mut count = 0;
        let mut bytes = 0;
        for input in inputs.clone() {
            debug_assert_eq!(input.slot, slot, "a packet holds one player's inputs");
            chain.extend(input.stamp, input.payload);
            if input.payload.len() > self.header.terms.max_payload_len as usize {
                return Err(InputError::PayloadTooLarge);
            }
            count += 1;
            bytes += input.payload.len();
        }
        let max_inputs = self.header.terms.max_inputs_per_tick as usize;
        if self.sent_this_tick[player] as usize + count > max_inputs {
            return Err(InputError::TooManyInputs);
        }
        if self.inputs.len() + count > self.position_bound
            || self.payloads.len() + bytes > self.position_bound
        {
            return Err(InputError::LogFull);
        }
        let session_key = &self.header.players[player].terms().session_key;
        if !chain.signed_by(&self.secp, session_key, self.session_id, signature) {
            return Err(InputError::BadSignature);
        }

        self.chains[player] = chain;
        self.sent_this_tick[player] += u32::try_from(count).expect("within the max per tick");
        applied.clear();
        applied.reserve_exact(count);
        for input in inputs {
            let index = offset(self.inputs.len());
            let payload_start = offset(self.payloads.len());
            self.payloads.extend_from_slice(input.payload);
            self.inputs.push(LoggedInput {
                slot: input.slot,
                stamp: input.stamp,
                payload: payload_start..offset(self.payloads.len()),
            });
            applied.push(self.schedule(index));
        }
        self.packets.push(PacketEnd {
            end: offset(self.inputs.len()),
            signature: *signature,
        });
        Ok(())
    }

    /// Closes the next tick to new inputs and gives the inputs applied in it, by slot, then in
    /// each player's chain order.
    pub fn seal_tick(&mut self) -> impl ExactSizeIterator<Item = PlayerInput<'_>> {
        let tick = self.next_tick();
        let replayed = usize::try_from(tick).expect("tick fits usize");
        let end = match self.to_replay.get(replayed) {
            Some(&end) => {
                let start = self.tick_ends.last().copied().unwrap_or(0);
                for index in start..end {
                    self.schedule(index);
                }
                end
            }
            None => offset(self.inputs.len()),
        };
        self.tick_ends.push(end);
        if !self.to_replay.is_empty() && self.tick_ends.len() == self.to_replay.len() {
            // Caught up: the inputs logged after the last tick wait again, as they did.
            self.to_replay = Vec::new();
            for index in end..offset(self.inputs.len()) {
                self.schedule(index);
            }
        }
        self.sent_this_tick.fill(0);
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
    /// log is as it was. A log that replays takes no input.
    #[must_use]
    pub fn rewound(mut self) -> SessionLog {
        if self.tick_ends.is_empty() {
            return self;
        }
        self.to_replay = mem::take(&mut self.tick_ends);
        self.pending.clear();
        self.due.clear();
        self.sent_this_tick.fill(0);
        self
    }

    /// Writes the log file into `out`, which is cleared first: the tag, then in postcard the
    /// header, the `u64` number of sealed ticks, the packets logged before each tick, the packets
    /// logged since the last tick, and the revealed seed as an option. Packets go as a `u32`
    /// count, then each as its `u32` slot, its inputs as a `u32` count and each input's stamp
    /// and payload bytes, and its signature. An input's seq and link are not written: a reader
    /// computes them from the chain, and the signature covers them. The header goes field by field,
    /// each player as the delegation's JSON.
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.clear();
        out.extend_from_slice(LOG_TAG);
        self.put_header(out);
        put(out, &self.next_tick());
        let mut start = 0;
        for &end in &self.tick_ends {
            self.put_packets(out, start..end);
            start = end;
        }
        self.put_packets(out, start..offset(self.inputs.len()));
        put(out, &self.revealed);
    }

    /// Decodes a log file. Its packets are recorded again, which checks every chain link and
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
        let terms = &self.header.terms;
        put(out, &terms.server_key);
        put(out, &terms.tick_hz);
        put(out, &terms.max_input_delay);
        put(out, &terms.max_input_lead);
        put(out, &terms.max_payload_len);
        put(out, &terms.max_inputs_per_tick);
        put(out, &terms.seed_commitment);
        put(out, terms.release.as_str());
        put(out, &terms.mode);
        put(out, &terms.dependencies);
        put(out, &offset(self.header.players.len()));
        for delegation in &self.header.players {
            put(out, delegation.json());
        }
    }

    /// Writes the packets whose inputs are `inputs`.
    fn put_packets(&self, out: &mut Vec<u8>, inputs: Range<u32>) {
        let packets = self.packets_within(inputs);
        put(out, &offset(packets.len()));
        for packet in packets {
            let slot = self.inputs[packet.inputs.start as usize].slot;
            put(out, &slot.get());
            put(out, &(packet.inputs.end - packet.inputs.start));
            for input in packet.inputs() {
                put(out, &input.stamp);
                put(out, input.payload);
            }
            put(out, &packet.signature);
        }
    }

    /// Records the packets `put_packets` wrote at the front of `rest` before the next tick, with
    /// `inputs` and `applied` as scratch.
    fn record_logged<'a>(
        &mut self,
        rest: &mut &'a [u8],
        inputs: &mut Vec<PlayerInput<'a>>,
        applied: &mut Vec<Applied>,
    ) -> Result<(), LogError> {
        let packets: u32 = take(rest)?;
        for _ in 0..packets {
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
            let tick = self.next_tick();
            self.record(inputs.iter().copied(), &signature, applied)
                .map_err(|error| LogError::Input { tick, error })?;
        }
        Ok(())
    }

    /// The packets whose inputs are `inputs`, which starts and ends at packet ends.
    fn packets_within(&self, inputs: Range<u32>) -> impl ExactSizeIterator<Item = Packet<'_>> {
        let first = self
            .packets
            .partition_point(|packet| packet.end <= inputs.start);
        let last = self
            .packets
            .partition_point(|packet| packet.end <= inputs.end);
        debug_assert!(
            first == last || self.packet_start(first) == inputs.start,
            "the inputs start at a packet"
        );
        (first..last).map(|index| Packet {
            log: self,
            inputs: self.packet_start(index)..self.packets[index].end,
            signature: self.packets[index].signature,
        })
    }

    fn packet_start(&self, index: usize) -> u32 {
        index
            .checked_sub(1)
            .map_or(0, |before| self.packets[before].end)
    }

    /// Queues the logged input `index`, logged before the next tick, for the tick it applies in,
    /// and says when that is.
    fn schedule(&mut self, index: u32) -> Applied {
        let logged = &self.inputs[index as usize];
        let (slot, outcome) = (logged.slot, self.applied(logged.stamp));
        if let Applied::At(tick) = outcome {
            self.pending.push(Reverse(Due { tick, slot, index }));
        }
        outcome
    }

    /// When an input stamped `stamp`, logged now, applies.
    fn applied(&self, stamp: u64) -> Applied {
        let next = self.next_tick();
        if next
            .checked_sub(stamp)
            .is_some_and(|delay| delay > self.header.terms.max_input_delay)
        {
            return Applied::Late;
        }
        if stamp
            .checked_sub(next)
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
    let terms = SessionTerms {
        server_key: take(rest)?,
        tick_hz: take(rest)?,
        max_input_delay: take(rest)?,
        max_input_lead: take(rest)?,
        max_payload_len: take(rest)?,
        max_inputs_per_tick: take(rest)?,
        seed_commitment: take(rest)?,
        release: take::<&str>(rest)?.to_owned(),
        mode: take(rest)?,
        dependencies: take(rest)?,
    };
    let count: u32 = take(rest)?;
    let mut players = Vec::new();
    for slot in 0..count {
        let json = take(rest)?;
        let delegation = Delegation::parse(json).map_err(|error| {
            LogError::Header(HeaderError::Delegation {
                slot: PlayerSlot::new(slot),
                error,
            })
        })?;
        players.push(delegation);
    }
    Ok(SessionHeader { terms, players })
}

#[cfg(test)]
mod tests;
