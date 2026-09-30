use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::ops::Range;

use blake3::Hasher;
use campfire_math::SegmentSeed;
use secp256k1::{Secp256k1, VerifyOnly};
use serde::{Deserialize, Serialize};

use crate::chain_signature::ChainSignature;
use crate::delegation::Delegation;
use crate::delegation::error::DelegationError;
use crate::input_chain::InputChain;
use crate::input_hash::InputHash;
use crate::player_input::PlayerInput;
use crate::player_slot::PlayerSlot;
use crate::server_seed::{SeedCommitment, ServerSeed};
use crate::session_id::SessionId;
use crate::session_log::error::{HeaderError, InputError, LogError, SeedError};

pub(crate) mod error;

/// Starts the segment seed, so no other BLAKE3 use can produce one.
const SEGMENT_SEED_DOMAIN: &[u8] = b"campfire/segment-seed/v1";
/// Starts every log file and states its protocol version, so other bytes are refused at once.
const LOG_TAG: &[u8] = b"campfire/session-log/v1";
/// The log's positions are `u32`: input indices, payload offsets and the count of inputs each
/// tick ends at.
const POSITION_BOUND: usize = u32::MAX as usize;

/// What the log fixes before the first tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHeader {
    pub session_id: SessionId,
    /// The server's x-only public key.
    pub server_key: [u8; 32],
    /// The most ticks an input may land after its stamp; a later one is logged as late.
    pub max_input_delay: u64,
    /// The most ticks an input's stamp may be ahead of the next tick; a further one is logged as
    /// early. The server holds each input until its tick, so this bounds what a client can make
    /// it hold.
    pub max_input_lead: u64,
    /// The most bytes an input's payload may hold.
    pub max_payload_len: u32,
    /// The most inputs a player may send before one tick. With the max payload length, it bounds
    /// how fast a player can grow the log.
    pub max_inputs_per_tick: u32,
    /// The server's commitment to its seed, made before the players sent their contributions.
    pub seed_commitment: SeedCommitment,
    /// The players, by slot.
    pub players: Vec<SessionPlayer>,
}

/// A player as the header lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPlayer {
    /// Lets the player's session key sign their chain heads; its id is what the player's first
    /// input links to.
    pub delegation: Delegation,
    /// The player's random share of the segment seed, sent after the server's commitment, so
    /// neither side alone chooses the seed.
    pub seed_contribution: [u8; 32],
}

impl SessionHeader {
    /// The segment seed, `BLAKE3(domain ‖ server seed ‖ contributions in slot order)`; an error
    /// when `server_seed` does not match the commitment.
    pub fn segment_seed(&self, server_seed: &ServerSeed) -> Result<SegmentSeed, SeedError> {
        if server_seed.commitment() != self.seed_commitment {
            return Err(SeedError::WrongSeed);
        }
        let mut hasher = Hasher::new();
        hasher
            .update(SEGMENT_SEED_DOMAIN)
            .update(server_seed.as_bytes());
        for player in &self.players {
            hasher.update(&player.seed_contribution);
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
/// after it, so every logged input is signed. The server records packets as they arrive; a
/// verifier records a published log again, which checks every chain link and signature and gives
/// each tick exactly the server's inputs.
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
    /// Inputs applied in a tick not yet sealed, earliest first.
    pending: BinaryHeap<Reverse<Due>>,
    /// Indices of the inputs applied in the tick last sealed.
    due: Vec<u32>,
    /// The most inputs and payload bytes the log holds: `POSITION_BOUND`, or less in a test.
    position_bound: usize,
}

#[derive(Debug)]
struct LoggedInput {
    slot: PlayerSlot,
    seq: u64,
    stamp: u64,
    previous: InputHash,
    payload: Range<u32>,
}

#[derive(Debug)]
struct PacketEnd {
    /// The end of the packet's inputs.
    end: u32,
    signature: ChainSignature,
}

/// Orders the inputs of one tick by slot, then seq, whatever order they arrived in, so the host
/// cannot choose who acts first.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Due {
    tick: u64,
    slot: PlayerSlot,
    seq: u64,
    index: u32,
}

/// A packet as the log holds it: one player's inputs, and the signature over the chain head after
/// the last.
#[derive(Debug, Clone)]
pub struct Packet<'a> {
    log: &'a SessionLog,
    inputs: Range<u32>,
    pub signature: ChainSignature,
}

impl<'a> Packet<'a> {
    pub fn inputs(&self) -> impl ExactSizeIterator<Item = PlayerInput<'a>> + Clone + use<'a> {
        let log = self.log;
        self.inputs.clone().map(move |index| log.input(index))
    }
}

impl SessionLog {
    /// A log with nothing recorded; an error when a player's delegation names another server or
    /// session than `header`, or there are more players than slots.
    pub fn new(header: SessionHeader) -> Result<SessionLog, HeaderError> {
        if u32::try_from(header.players.len()).is_err() {
            return Err(HeaderError::TooManyPlayers);
        }
        let mut chains = Vec::with_capacity(header.players.len());
        for (slot, player) in (0..).zip(&header.players) {
            let slot = PlayerSlot::new(slot);
            let terms = player.delegation.terms();
            let flaw = if terms.server_key != header.server_key {
                Some(DelegationError::OtherServer)
            } else if terms.session_id != header.session_id {
                Some(DelegationError::OtherSession)
            } else {
                None
            };
            if let Some(error) = flaw {
                return Err(HeaderError::Delegation { slot, error });
            }
            chains.push(InputChain::new(slot, player.delegation.chain_root()));
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
            pending: BinaryHeap::new(),
            due: Vec::new(),
            position_bound: POSITION_BOUND,
        })
    }

    pub const fn header(&self) -> &SessionHeader {
        &self.header
    }

    /// Adds the server seed, which publishes the segment.
    pub fn reveal_seed(&mut self, server_seed: ServerSeed) {
        assert!(
            server_seed.commitment() == self.header.seed_commitment,
            "the server reveals the seed it committed to"
        );
        self.revealed = Some(server_seed);
    }

    /// The server seed, once the segment is published.
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
        let mut chain = *self.chains.get(player).ok_or(InputError::UnknownPlayer)?;
        let mut count = 0;
        let mut bytes = 0;
        for input in inputs.clone() {
            debug_assert_eq!(input.slot, slot, "a packet holds one player's inputs");
            chain.accept(&input)?;
            if input.payload.len() > self.header.max_payload_len as usize {
                return Err(InputError::PayloadTooLarge);
            }
            count += 1;
            bytes += input.payload.len();
        }
        if self.sent_this_tick[player] as usize + count > self.header.max_inputs_per_tick as usize {
            return Err(InputError::TooManyInputs);
        }
        if self.inputs.len() + count > self.position_bound
            || self.payloads.len() + bytes > self.position_bound
        {
            return Err(InputError::LogFull);
        }
        let session_key = &self.header.players[player].delegation.terms().session_key;
        if !chain.signed_by(&self.secp, session_key, self.header.session_id, signature) {
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
                seq: input.seq,
                stamp: input.stamp,
                previous: input.previous,
                payload: payload_start..offset(self.payloads.len()),
            });
            let outcome = self.applied(input.stamp);
            if let Applied::At(tick) = outcome {
                self.pending.push(Reverse(Due {
                    tick,
                    slot: input.slot,
                    seq: input.seq,
                    index,
                }));
            }
            applied.push(outcome);
        }
        self.packets.push(PacketEnd {
            end: offset(self.inputs.len()),
            signature: *signature,
        });
        Ok(())
    }

    /// Closes the next tick to new inputs and gives the inputs applied in it, by slot, then seq.
    pub fn seal_tick(&mut self) -> impl ExactSizeIterator<Item = PlayerInput<'_>> {
        let tick = self.next_tick();
        self.tick_ends.push(offset(self.inputs.len()));
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

    /// The packets logged before the sealed `tick` ran, in the order they arrived.
    pub fn packets_before(&self, tick: u64) -> impl ExactSizeIterator<Item = Packet<'_>> {
        let tick = usize::try_from(tick).expect("tick fits usize");
        let start = tick
            .checked_sub(1)
            .map_or(0, |before| self.tick_ends[before]);
        self.packets_within(start..self.tick_ends[tick])
    }

    /// Writes the log file into `out`, which is cleared first: the tag, then in postcard the
    /// header, the `u64` number of sealed ticks, the packets logged before each tick, the packets
    /// logged since the last tick, and the revealed seed as an option. Packets go as a `u32`
    /// count, then each as its `u32` slot, its inputs as a `u32` count and each input's seq,
    /// stamp, previous hash and payload bytes, and its signature. The header goes field by field,
    /// each player as the delegation's JSON and the seed contribution.
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
            if server_seed.commitment() != log.header.seed_commitment {
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
        let header = &self.header;
        put(out, header.session_id.as_bytes());
        put(out, &header.server_key);
        put(out, &header.max_input_delay);
        put(out, &header.max_input_lead);
        put(out, &header.max_payload_len);
        put(out, &header.max_inputs_per_tick);
        put(out, &header.seed_commitment);
        put(out, &offset(header.players.len()));
        for player in &header.players {
            put(out, player.delegation.json());
            put(out, &player.seed_contribution);
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
                put(out, &input.seq);
                put(out, &input.stamp);
                put(out, &input.previous);
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
                let seq = take(rest)?;
                let stamp = take(rest)?;
                let previous = take(rest)?;
                let payload = take(rest)?;
                inputs.push(PlayerInput {
                    slot,
                    seq,
                    stamp,
                    previous,
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

    /// When an input stamped `stamp`, logged now, applies.
    fn applied(&self, stamp: u64) -> Applied {
        let next = self.next_tick();
        if next
            .checked_sub(stamp)
            .is_some_and(|delay| delay > self.header.max_input_delay)
        {
            return Applied::Late;
        }
        if stamp
            .checked_sub(next)
            .is_some_and(|lead| lead > self.header.max_input_lead)
        {
            return Applied::Early;
        }
        Applied::At(stamp.max(next))
    }

    fn input(&self, index: u32) -> PlayerInput<'_> {
        let logged = &self.inputs[index as usize];
        PlayerInput {
            slot: logged.slot,
            seq: logged.seq,
            stamp: logged.stamp,
            previous: logged.previous,
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
    let session_id = SessionId::new(take(rest)?);
    let server_key = take(rest)?;
    let max_input_delay = take(rest)?;
    let max_input_lead = take(rest)?;
    let max_payload_len = take(rest)?;
    let max_inputs_per_tick = take(rest)?;
    let seed_commitment = take(rest)?;
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
        players.push(SessionPlayer {
            delegation,
            seed_contribution: take(rest)?,
        });
    }
    Ok(SessionHeader {
        session_id,
        server_key,
        max_input_delay,
        max_input_lead,
        max_payload_len,
        max_inputs_per_tick,
        seed_commitment,
        players,
    })
}

#[cfg(test)]
mod tests;
