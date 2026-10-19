//! The log's file: its encode, and the decode that records each entry again.

use std::ops::Range;

use campfire_codec::BinaryError;
use campfire_common::{PlayerSlot, Tick};

use crate::bytes::Bytes;
use crate::checkpoint::Checkpoint;
use crate::checkpoint::error::CheckpointDecodeError;
use crate::delegation::Delegation;
use crate::server_input::ServerInput;
use crate::server_input::error::ServerInputDecodeError;
use crate::server_seed::ServerSeed;
use crate::session_log::applied::Applied;
use crate::session_log::error::{HeaderError, LogError};
use crate::session_log::log_entries::Entry;
use crate::session_log::player_packet::PlayerPacket;
use crate::session_log::session_header::SessionHeader;
use crate::session_log::{Bounds, LoggedResult, Past, SessionLog, len, offset, put, take};
use crate::session_terms::SessionTerms;
use crate::slot_plan::SlotPlan;
use crate::slot_start::SlotStart;

/// Starts every log file and states its protocol version, so other bytes are refused at once.
pub(super) const LOG_TAG: &[u8] = b"campfire/session-log/v6";
/// The bytes that start a player's packet and a server input in the file.
pub(super) const PACKET_ENTRY: u8 = 0;
pub(super) const SERVER_ENTRY: u8 = 1;

impl SessionLog {
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
        debug_assert!(
            self.holds_all(),
            "a log is written whole only while it forgot nothing, as one with no journal"
        );
        assert!(
            self.segments.begun_count() == 0,
            "a log is written once every checkpoint it began has its record"
        );
        let start = self.put_segments(out, self.segments.len(), self.next_tick());
        self.put_entries(out, start..self.entries.len());
        put(out, &self.revealed);
        put(out, &self.result);
        debug_assert_eq!(
            len(out.len()),
            self.file_len,
            "the log counts its file's bytes"
        );
    }

    /// Writes into `out`, which is cleared first, the tag, the header, and the first `segments`
    /// segments, the last ending before `end`; where the entries of the ticks written end.
    fn put_segments(&self, out: &mut Vec<u8>, segments: usize, end: Tick) -> u32 {
        out.clear();
        out.extend_from_slice(LOG_TAG);
        self.put_header(out);
        put(out, &offset(segments));
        let mut start = 0;
        let written = self.segments.first(segments);
        for (place, segment) in written.iter().enumerate() {
            if let Some(logged) = &segment.checkpoint {
                logged.record.encode(out);
                put(out, &logged.signature);
            }
            let end = written.get(place + 1).map_or(end, |next| next.tick);
            put(out, &(end.get() - segment.tick.get()));
            for tick in segment.tick.get()..end.get() {
                let tick_end = self.entries.tick_end(Tick::new(tick));
                self.put_entries(out, start..tick_end);
                start = tick_end;
            }
        }
        start
    }

    /// Decodes a log file. Its entries are recorded again, which checks every chain link and
    /// signature, and only the bytes `encode` gives for the decoded log are accepted, so a log has
    /// one file.
    pub fn decode(bytes: &[u8]) -> Result<Self, LogError> {
        Self::decode_within(bytes, Bounds::MAX)
    }

    /// Decodes a log file as `decode` does, the log's file and journal held within `bounds`. Each
    /// value it takes is canonical postcard, and each count is the count of what follows, so the
    /// bytes `encode` gives for the decoded log are the file's, which a debug build checks.
    pub(super) fn decode_within(bytes: &[u8], bounds: Bounds) -> Result<Self, LogError> {
        let mut rest = bytes.strip_prefix(LOG_TAG).ok_or(LogError::NotLog)?;
        let header = take_header(&mut rest)?;
        let mut log = Self::within(header, bounds).map_err(LogError::Header)?;
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
                drop(log.seal_tick(None));
            }
        }
        log.record_logged(&mut rest, &mut inputs, &mut applied)?;
        if let Some(server_seed) = take::<Option<ServerSeed>>(&mut rest)? {
            if !server_seed.check(log.segment(), &log.header.terms.seed_commitment) {
                return Err(LogError::WrongSeed);
            }
            log.set_revealed(server_seed);
        }
        if let Some(LoggedResult { result, signature }) = take(&mut rest)? {
            log.record_result(result, &signature, None)
                .map_err(LogError::Result)?;
        }
        if !rest.is_empty() {
            return Err(LogError::Trailing);
        }
        if cfg!(debug_assertions) {
            let mut canonical = Vec::with_capacity(bytes.len());
            log.encode(&mut canonical);
            assert!(
                canonical == bytes,
                "a file that decodes is the file its log encodes"
            );
        }
        Ok(log)
    }

    pub(super) fn put_header(&self, out: &mut Vec<u8>) {
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
        for entry in self.entries.entries(entries) {
            match entry {
                Entry::Packet(index) => {
                    put(out, &PACKET_ENTRY);
                    self.put_packet(out, index);
                }
                Entry::Server(index) => {
                    put(out, &SERVER_ENTRY);
                    self.put_server(out, index);
                }
            }
        }
    }

    /// Writes the packet `index`, as `write_packet` does.
    pub(super) fn put_packet(&self, out: &mut Vec<u8>, index: u32) {
        let packet = self.entries.packet(index);
        let slot = self.entries.input(packet.inputs.start).slot;
        let inputs = packet.inputs.clone().map(|index| {
            let input = self.entries.input(index);
            (input.stamp, input.payload)
        });
        let written = PlayerPacket {
            slot,
            inputs,
            signature: &packet.signature,
        };
        write_packet(out, written);
    }

    /// Writes the server input `index` and its signature.
    pub(super) fn put_server(&self, out: &mut Vec<u8>, index: u32) {
        let logged = self.entries.server(index);
        self.entries.server_input(logged.served).encode(out);
        put(out, &logged.signature);
    }

    /// Records the entries `put_entries` wrote at the front of `rest` before the next tick, with
    /// `inputs` and `applied` as scratch.
    fn record_logged<'a>(
        &mut self,
        rest: &mut &'a [u8],
        inputs: &mut Vec<(Tick, &'a [u8])>,
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
    pub(super) fn record_packet<'a>(
        &mut self,
        rest: &mut &'a [u8],
        inputs: &mut Vec<(Tick, &'a [u8])>,
        applied: &mut Vec<Applied>,
    ) -> Result<(), LogError> {
        let tick = self.next_tick();
        let slot = PlayerSlot::new(take(rest)?);
        let count: u32 = take(rest)?;
        inputs.clear();
        for _ in 0..count {
            let stamp = take(rest)?;
            let payload = take(rest)?;
            inputs.push((stamp, payload));
        }
        let signature = take(rest)?;
        let packet = PlayerPacket {
            slot,
            inputs: inputs.iter().copied(),
            signature: &signature,
        };
        self.record(packet, applied, None)
            .map_err(|error| LogError::Input { tick, error })
    }

    /// Records the server input `put_server` wrote at the front of `rest`.
    pub(super) fn record_server_input(&mut self, rest: &mut &[u8]) -> Result<(), LogError> {
        let tick = self.next_tick();
        let taken = ServerInput::take(rest).map_err(|error| match error {
            ServerInputDecodeError::Malformed(BinaryError::Truncated) => LogError::Truncated,
            error => LogError::ServerDecode { tick, error },
        })?;
        *rest = taken.rest;
        let signature = take(rest)?;
        self.record_server(taken.value, &signature, None)
            .map_err(|error| LogError::Server { tick, error })
    }

    /// Begins the checkpoint of segment `segment`, and records its record that `encode` wrote at
    /// the front of `rest`, with its signature.
    fn record_checkpoint_at(&mut self, rest: &mut &[u8], segment: u32) -> Result<(), LogError> {
        self.begin_checkpoint(None)
            .map_err(|error| LogError::Checkpoint { segment, error })?;
        self.record_done(rest, segment, Past::InLog)
    }

    /// Records the record of the checkpoint begun, of segment `segment`, that `encode` wrote at
    /// the front of `rest`, with its signature, the past before its boundary held where `past`
    /// says.
    pub(super) fn record_done(
        &mut self,
        rest: &mut &[u8],
        segment: u32,
        past: Past,
    ) -> Result<(), LogError> {
        let taken = Checkpoint::take(rest).map_err(|error| match error {
            CheckpointDecodeError::Malformed(BinaryError::Truncated) => LogError::Truncated,
            error => LogError::CheckpointDecode { segment, error },
        })?;
        *rest = taken.rest;
        let signature = take(rest)?;
        self.record_checkpoint_in(taken.value, &signature, None, past)
            .map_err(|error| LogError::Checkpoint { segment, error })
    }
}

/// Writes `packet`: its `u32` slot, its inputs as a `u32` count and each input's stamp and
/// payload bytes, and its signature.
pub(super) fn write_packet<'a>(
    out: &mut Vec<u8>,
    packet: PlayerPacket<'_, impl Iterator<Item = (Tick, &'a [u8])> + Clone>,
) {
    let count = u32::try_from(packet.inputs.clone().count()).expect("a packet's inputs fit u32");
    put(out, &packet.slot.get());
    put(out, &count);
    for (stamp, payload) in packet.inputs {
        put(out, &stamp);
        put(out, &Bytes(payload));
    }
    put(out, packet.signature);
}

/// Reads the header `SessionLog::put_header` wrote, checking each delegation.
pub(super) fn take_header(rest: &mut &[u8]) -> Result<SessionHeader, LogError> {
    let terms: SessionTerms = take(rest)?;
    let count: u32 = take(rest)?;
    let mut slots = Vec::new();
    for slot in 0..count {
        let plan = SlotPlan::try_from(take::<u8>(rest)?).map_err(|_| {
            LogError::Header(HeaderError::UnknownStart {
                slot: PlayerSlot::new(slot),
            })
        })?;
        let start = match plan {
            SlotPlan::Player => {
                let json = take(rest)?;
                let delegation = Delegation::parse(json).map_err(|error| {
                    LogError::Header(HeaderError::Delegation {
                        slot: PlayerSlot::new(slot),
                        error,
                    })
                })?;
                SlotStart::player(delegation)
            }
            SlotPlan::Bot => SlotStart::Bot,
            SlotPlan::Open => SlotStart::Open,
        };
        slots.push(start);
    }
    Ok(SessionHeader { terms, slots })
}
