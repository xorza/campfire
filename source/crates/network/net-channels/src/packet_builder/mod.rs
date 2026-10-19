//! The packing of a link's due messages into packets, to the packet session's budget
//! (13-net-stack/transport.md, A flush stops at the packet session's budget).
use crate::bench_counter::BenchCounter;
use crate::compression::{CompressionOutcome, PacketCompression};
use crate::compression_scratch::CompressionScratch;
use crate::compression_setup::CompressionSetup;
use crate::message_ref::MessageRef;
use crate::packet::{DEFAULT_MAX_PAYLOAD, Packet, PacketCompressionInfo};
use crate::packet_buffer_pool::PacketBufferPool;
use crate::packet_error::PacketError;
use crate::packet_kind::PacketKind;
use crate::packet_message::PacketMessage;
use crate::packet_message::SendCommit;
use crate::send_candidate::{CandidateMessages, SendCandidate};
use crate::send_message_key::SendMessageKey;
use crate::single_body::SingleBody;
use crate::tick_encoder::EncodedTick;
use bytes::BytesMut;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use campfire_net_core::NetId;
use campfire_net_payload::OutgoingPayload;
use tracing::trace;

const MAX_MESSAGES_PER_CHANNEL_BATCH: u8 = u8::MAX;

/// Builder-local state for the currently open batch of single messages.
///
/// A batch is encoded as a channel ID, a one-byte message count, and that many consecutive
/// [`SingleBody`](crate::single_body::SingleBody) values. Grouping messages this way avoids
/// repeating the channel ID for every message. This state is not serialized as a struct: `count`
/// mirrors the serialized count byte at `count_offset` so the builder can update it in place.
/// Once `count` reaches [`u8::MAX`], another batch is started, even for the same channel.
#[derive(Clone, Copy, Debug)]
struct SingleBatchState {
    /// Channel shared by every message in this batch.
    channel_id: NetId,
    /// Offset of the serialized message-count byte in the packet payload.
    count_offset: usize,
    /// Number of messages in this batch and the value stored at `count_offset`.
    count: u8,
}

/// A packet that the builder still appends single messages to.
#[derive(Debug)]
struct OpenPacket {
    packet: Packet,
    batch: Option<SingleBatchState>,
    /// The last state known to fit: its payload within the limit, or its body compressed into
    /// [`PacketFit::kept`] within the room.
    verified: Mark,
    /// The compression of the verified body, once a body passed the limit.
    kept: Option<KeptCompression>,
    /// The payload length past which an append compresses the body again: the limit, then where
    /// the last compression's ratio fills the room.
    bound: usize,
}

/// A state of an [`OpenPacket`] that an append can roll back to.
#[derive(Debug, Clone, Copy)]
struct Mark {
    payload_len: usize,
    messages_len: usize,
    batch: Option<SingleBatchState>,
}

/// A compression that fits the room, of a body of `original_len` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeptCompression {
    original_len: usize,
    compressed_len: usize,
}

/// What decides whether a packet fits: its size limit and, with compression, the size its body
/// compresses to.
#[derive(Debug)]
struct PacketFit<'a> {
    compression: PacketCompression,
    /// The largest payload buffer, its headroom included.
    limit: usize,
    /// The bytes before the body: the headroom and the tick.
    header_len: usize,
    scratch: &'a mut CompressionScratch,
    /// The compressed body of the payload that [`OpenPacket::kept`] describes.
    kept: &'a mut Vec<u8>,
    /// The compressed body of a payload being tried.
    trial: &'a mut Vec<u8>,
}

/// What one send flush stages its packets from.
#[derive(Debug)]
pub(crate) struct PacketSource<'a, M> {
    /// The flush's tick, as each of its packets carries it.
    pub(crate) tick: EncodedTick,
    /// The candidates in priority and packing order.
    pub(crate) candidates: &'a [SendCandidate],
    pub(crate) messages: &'a M,
    pub(crate) compression: CompressionSetup,
    /// The largest payload one packet carries, after the packet session's headroom.
    pub(crate) max_payload: usize,
}

/// `PacketBuilder` packs the messages of a flush into packets.
#[derive(Debug)]
pub(crate) struct PacketBuilder {
    buffer_pool: PacketBufferPool,
    /// The payload of a packet of single messages while it fills. With compression it can pass
    /// the largest payload, so it is one buffer that grows to the largest such payload, and the
    /// pooled payloads stay at theirs.
    staging: BytesMut,
    compression_output: Vec<u8>,
    compression_trial: Vec<u8>,
}

impl Default for PacketBuilder {
    fn default() -> Self {
        Self {
            buffer_pool: PacketBufferPool::new(Self::buffer_capacity(DEFAULT_MAX_PAYLOAD)),
            staging: BytesMut::new(),
            compression_output: Vec::new(),
            compression_trial: Vec::new(),
        }
    }
}

impl PacketBuilder {
    /// The buffer of a packet of at most `max_payload` bytes: the packet session's headroom before
    /// it, where the packet session writes its header in place, and its tailroom after, where the
    /// seal appends its tag.
    const fn buffer_capacity(max_payload: usize) -> usize {
        OutgoingPayload::HEADROOM + max_payload + OutgoingPayload::TAILROOM
    }

    pub(crate) fn recycle_packet(&mut self, packet: Packet) {
        self.buffer_pool.recycle_packet(packet);
    }

    pub(crate) fn recycle_packet_messages(&mut self, messages: Vec<PacketMessage>) {
        self.buffer_pool.recycle_packet_messages(messages);
    }

    /// Prepares the packet buffer pool for one transport send pass of packets of at most
    /// `max_payload` bytes.
    pub(crate) fn begin_send(&mut self, max_payload: usize) {
        self.buffer_pool
            .set_payload_capacity(Self::buffer_capacity(max_payload));
    }

    /// Stage the next packet from globally ordered, channel-held candidates.
    ///
    /// This method only reads the candidates and their messages. It deliberately does not mutate
    /// channel queues, reliable send state or the packet session's budget. The caller commits those
    /// transitions after the returned packet goes to the packet session.
    pub(crate) fn build_next_packet<M: CandidateMessages>(
        &mut self,
        source: &PacketSource<'_, M>,
        cursor: &mut CandidateCursor,
        compression_scratch: &mut CompressionScratch,
    ) -> Result<Option<Packet>, PacketError> {
        let PacketSource {
            tick,
            candidates,
            messages,
            compression,
            max_payload,
        } = *source;
        let header_len = OutgoingPayload::HEADROOM + tick.as_bytes().len();
        let limit = OutgoingPayload::HEADROOM + max_payload;
        let room = limit.saturating_sub(header_len);
        // Final fragments can consume singles which appear later in their priority group. Skip
        // those candidates once the main cursor reaches the single portion of the group.
        while cursor.index < cursor.single_index
            && candidates
                .get(cursor.index)
                .is_some_and(SendCandidate::is_single)
        {
            cursor.index += 1;
        }
        let Some(first) = candidates.get(cursor.index) else {
            return Ok(None);
        };
        // A fragmented message's candidate stays current while its due fragments go, one in
        // each packet.
        let key = cursor.fragment.take().unwrap_or(first.key);

        match messages.message(first.channel_id, key) {
            MessageRef::Fragment(fragment) => {
                let priority = first.rank.priority;
                let mut open = OpenPacket::new(
                    self.new_staged_packet(PacketKind::DataFragment, tick),
                    limit,
                );
                first.channel_id.to_bytes(&mut open.packet.payload);
                fragment.write(&mut open.packet.payload);
                open.packet.record_message(PacketMessage {
                    commit: SendCommit {
                        channel_id: first.channel_id,
                        key,
                    },
                    len: BenchCounter::new(fragment.payload().len()),
                });
                cursor.fragment = messages.next_due(first.channel_id, key);
                if cursor.fragment.is_none() {
                    cursor.index += 1;
                }

                // The wire format permits singles after the final fragment, which fill it
                // uncompressed. The separate single cursor can look past other fragments in the
                // same priority group, letting every fragmented message use its final packet's
                // remaining space.
                if fragment.is_last() {
                    let mut fit = PacketFit {
                        compression: PacketCompression {
                            setup: CompressionSetup::DISABLED,
                            room,
                        },
                        limit,
                        header_len,
                        scratch: compression_scratch,
                        kept: &mut self.compression_output,
                        trial: &mut self.compression_trial,
                    };
                    cursor.single_index = cursor.single_index.max(cursor.index);
                    while let Some(candidate) = candidates.get(cursor.single_index) {
                        if candidate.rank.priority.total_cmp(&priority)
                            != core::cmp::Ordering::Equal
                        {
                            break;
                        }
                        let MessageRef::Single(message) =
                            messages.message(candidate.channel_id, candidate.key)
                        else {
                            cursor.single_index += 1;
                            continue;
                        };
                        if !open.try_append_single(candidate, message, &mut fit)? {
                            break;
                        }
                        cursor.single_index += 1;
                    }
                }

                if open.packet.payload.len() > limit {
                    return Err(PacketError::PacketTooLarge {
                        actual: open.packet.len(),
                        max: max_payload,
                    });
                }
                Ok(Some(open.packet))
            }
            MessageRef::Single(_) => {
                // Without compression the payload is the pooled buffer itself, which an append
                // never grows past the limit; with it, a body may pass the limit while it fills,
                // so it fills the staging buffer, which grows once to the largest.
                let compressing = compression.is_enabled();
                let mut payload = if compressing {
                    let mut staging = core::mem::take(&mut self.staging);
                    staging.clear();
                    staging
                } else {
                    self.buffer_pool.take_payload()
                };
                payload.resize(OutgoingPayload::HEADROOM, 0);
                payload.extend_from_slice(tick.as_bytes());
                let mut open = OpenPacket::new(
                    Packet {
                        payload,
                        kind: PacketKind::Data,
                        messages: self.buffer_pool.take_packet_messages(),
                        compression: None,
                        compression_abandoned: false,
                    },
                    limit,
                );
                let mut fit = PacketFit {
                    compression: PacketCompression {
                        setup: compression,
                        room,
                    },
                    limit,
                    header_len,
                    scratch: compression_scratch,
                    kept: &mut self.compression_output,
                    trial: &mut self.compression_trial,
                };
                let first_index = cursor.index;
                let mut appended = Ok(());
                while let Some(candidate) = candidates.get(cursor.index) {
                    let MessageRef::Single(message) =
                        messages.message(candidate.channel_id, candidate.key)
                    else {
                        break;
                    };
                    match open.try_append_single(candidate, message, &mut fit) {
                        Ok(true) => cursor.index += 1,
                        Ok(false) => break,
                        Err(error) => {
                            appended = Err(error);
                            break;
                        }
                    }
                }
                let sealed = appended.and_then(|()| {
                    if open.packet.messages.is_empty() {
                        let candidate = &candidates[cursor.index];
                        return Err(PacketError::PacketTooLarge {
                            actual: tick.as_bytes().len()
                                + candidate.channel_id.bytes_len()
                                + 1
                                + messages
                                    .message(candidate.channel_id, candidate.key)
                                    .bytes_len(),
                            max: max_payload,
                        });
                    }
                    Ok(())
                });
                let packet = if compressing {
                    let payload = self.buffer_pool.take_payload();
                    let Sealed { packet, staging } = open.seal(payload, &mut fit, sealed);
                    self.staging = staging;
                    packet
                } else {
                    sealed.map(|()| open.packet)
                };
                // A seal that rolled back to the verified state leaves the later messages to the
                // next packet.
                if let Ok(packet) = &packet {
                    cursor.index = first_index + packet.messages.len();
                }
                cursor.single_index = cursor.single_index.max(cursor.index);
                packet.map(Some)
            }
        }
    }

    /// A packet of the tick alone, which an idle transport sends.
    pub(crate) fn build_tick_packet(&mut self, tick: EncodedTick) -> Packet {
        self.new_staged_packet(PacketKind::Data, tick)
    }

    fn new_staged_packet(&mut self, kind: PacketKind, tick: EncodedTick) -> Packet {
        let mut payload = self.buffer_pool.take_payload();
        payload.resize(OutgoingPayload::HEADROOM, 0);
        payload.extend_from_slice(tick.as_bytes());
        Packet {
            payload,
            kind,
            messages: self.buffer_pool.take_packet_messages(),
            compression: None,
            compression_abandoned: false,
        }
    }
}

impl OpenPacket {
    fn new(packet: Packet, limit: usize) -> Self {
        let verified = Mark {
            payload_len: 0,
            messages_len: 0,
            batch: None,
        };
        let mut open = Self {
            packet,
            batch: None,
            verified,
            kept: None,
            bound: limit,
        };
        open.verified = open.mark();
        open
    }

    fn mark(&self) -> Mark {
        Mark {
            payload_len: self.packet.payload.len(),
            messages_len: self.packet.messages.len(),
            batch: self.batch,
        }
    }

    fn roll_back(&mut self, to: Mark) {
        if let Some(batch) = to.batch {
            self.packet.payload.as_mut()[batch.count_offset] = batch.count;
        }
        self.packet.payload.truncate(to.payload_len);
        self.packet.messages.truncate(to.messages_len);
        self.batch = to.batch;
    }

    /// Appends `message` when the packet may still fit with it, and leaves the packet unchanged
    /// otherwise.
    ///
    /// Without compression, a message that does not fit is not written. With it, the body is
    /// compressed only when it passes the bound, which each compression that fits moves to where
    /// its ratio fills the room, so a packet takes a few compressions, not one for each message
    /// past the limit; an append below the bound is checked when the packet seals.
    fn try_append_single(
        &mut self,
        candidate: &SendCandidate,
        message: &SingleBody,
        fit: &mut PacketFit<'_>,
    ) -> Result<bool, PacketError> {
        let opens_batch = !self.batch.is_some_and(|state| {
            state.channel_id == candidate.channel_id && state.count < MAX_MESSAGES_PER_CHANNEL_BATCH
        });
        let append_len = if opens_batch {
            candidate.channel_id.bytes_len() + 1
        } else {
            0
        } + message.bytes_len();
        let compressing = fit.compression.setup.is_enabled();
        if !compressing && self.packet.payload.len() + append_len > fit.limit {
            return Ok(false);
        }

        let before = self.mark();
        let packet = &mut self.packet;
        match &mut self.batch {
            Some(state) if !opens_batch => {
                state.count += 1;
                packet.payload.as_mut()[state.count_offset] = state.count;
            }
            batch => {
                candidate.channel_id.to_bytes(&mut packet.payload);
                let count_offset = packet.payload.len();
                packet.payload.write_u8(1);
                *batch = Some(SingleBatchState {
                    channel_id: candidate.channel_id,
                    count_offset,
                    count: 1,
                });
            }
        }
        message.write(&mut packet.payload);
        packet.record_message(PacketMessage {
            commit: SendCommit {
                channel_id: candidate.channel_id,
                key: candidate.key,
            },
            len: BenchCounter::new(message.bytes.len()),
        });

        let len = packet.payload.len();
        if len <= fit.limit {
            self.verified = self.mark();
            return Ok(true);
        }
        if len <= self.bound {
            return Ok(true);
        }
        let CompressionOutcome::Compressed {
            original_len,
            compressed_len,
        } = fit
            .compression
            .evaluate(&packet.payload[fit.header_len..], fit.scratch, fit.trial)?
        else {
            self.roll_back(before);
            return Ok(false);
        };
        core::mem::swap(fit.kept, fit.trial);
        self.kept = Some(KeptCompression {
            original_len,
            compressed_len,
        });
        self.verified = self.mark();
        self.bound = fit.header_len + original_len * fit.compression.room / compressed_len;
        Ok(true)
    }
}

/// A sealed packet of single messages, and the staging buffer it leaves.
#[derive(Debug)]
struct Sealed {
    packet: Result<Packet, PacketError>,
    staging: BytesMut,
}

impl OpenPacket {
    /// Moves the staged payload into `payload`, its body compressed when that fits, unless
    /// `staged` failed. A body compressed while it filled is not compressed again; one past the
    /// limit whose compression does not fit rolls back to the verified state.
    fn seal(
        mut self,
        mut payload: BytesMut,
        fit: &mut PacketFit<'_>,
        staged: Result<(), PacketError>,
    ) -> Sealed {
        let outcome = staged.and_then(|()| self.seal_compression(fit));
        let packet = outcome.map(|outcome| {
            let staging = &self.packet.payload;
            let (kind, compression) = match outcome {
                SealCompression::Kept(kept) => {
                    payload.extend_from_slice(&staging[..fit.header_len]);
                    payload.extend_from_slice(&fit.kept[..kept.compressed_len]);
                    (PacketKind::DataCompressed, Some(kept))
                }
                SealCompression::Trial(trial) => {
                    payload.extend_from_slice(&staging[..fit.header_len]);
                    payload.extend_from_slice(&fit.trial[..trial.compressed_len]);
                    (PacketKind::DataCompressed, Some(trial))
                }
                SealCompression::None { .. } => {
                    payload.extend_from_slice(staging);
                    (PacketKind::Data, None)
                }
            };
            trace!(
                target: "campfire_net_debug::transport",
                kind = "packet_compression",
                ?outcome,
                bytes = staging.len(),
                messages = self.packet.messages.len(),
                "transport packet compression outcome"
            );
            Packet {
                payload,
                kind,
                messages: core::mem::take(&mut self.packet.messages),
                compression: compression.map(|kept| PacketCompressionInfo {
                    original_len: kept.original_len,
                    compressed_len: kept.compressed_len,
                }),
                compression_abandoned: matches!(outcome, SealCompression::None { abandoned: true }),
            }
        });
        let mut staging = self.packet.payload;
        staging.clear();
        Sealed { packet, staging }
    }

    /// How the packet goes: with the kept compression, with a new one, or uncompressed, after a
    /// roll back to the verified state when neither fits.
    fn seal_compression(
        &mut self,
        fit: &mut PacketFit<'_>,
    ) -> Result<SealCompression, PacketError> {
        let body_len = self.packet.payload.len() - fit.header_len;
        if let Some(kept) = self.kept
            && kept.original_len == body_len
        {
            return Ok(SealCompression::Kept(kept));
        }
        let outcome = fit.compression.evaluate(
            &self.packet.payload[fit.header_len..],
            fit.scratch,
            fit.trial,
        )?;
        if let CompressionOutcome::Compressed {
            original_len,
            compressed_len,
        } = outcome
        {
            return Ok(SealCompression::Trial(KeptCompression {
                original_len,
                compressed_len,
            }));
        }
        let abandoned = matches!(outcome, CompressionOutcome::NotSmaller { .. });
        if self.packet.payload.len() <= fit.limit {
            return Ok(SealCompression::None { abandoned });
        }
        self.roll_back(self.verified);
        Ok(match self.kept {
            Some(kept) if kept.original_len == self.verified.payload_len - fit.header_len => {
                SealCompression::Kept(kept)
            }
            _ => SealCompression::None { abandoned },
        })
    }
}

/// The compression a sealed packet goes with.
#[derive(Debug, Clone, Copy)]
enum SealCompression {
    /// The one [`PacketFit::kept`] holds.
    Kept(KeptCompression),
    /// The one [`PacketFit::trial`] holds.
    Trial(KeptCompression),
    /// None, as the body did not compress smaller within the room when `abandoned`.
    None { abandoned: bool },
}

/// Position in the ordered send-candidate scratch for the current flush.
#[derive(Debug, Default)]
pub(crate) struct CandidateCursor {
    pub(crate) index: usize,
    pub(crate) single_index: usize,
    /// The next fragment of the fragmented message at `index`, past its candidate's first.
    pub(crate) fragment: Option<SendMessageKey>,
}

#[cfg(test)]
mod tests;
