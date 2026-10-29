//! The packing of a link's due messages into packets, to the packet session's budget
//! (11-net-stack/transport.md, A flush stops at the packet session's budget).
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use campfire_net_core::NetId;
use campfire_net_payload::OutgoingPayload;

use crate::bench_counter::BenchCounter;
use crate::message_ref::MessageRef;
use crate::packet::{DEFAULT_MAX_PAYLOAD, Packet};
use crate::packet_buffer_pool::PacketBufferPool;
use crate::packet_kind::PacketKind;
use crate::packet_message::PacketMessage;
use crate::packet_message::SendCommit;
use crate::send_candidate::{CandidateMessages, SendCandidate};
use crate::send_message_key::SendMessageKey;
use crate::single_body::SingleBody;
use crate::tick_encoder::EncodedTick;

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
    /// The largest payload buffer, its headroom included.
    limit: usize,
}

/// What one send flush stages its packets from.
#[derive(Debug)]
pub(crate) struct PacketSource<'a, M> {
    /// The flush's tick, as each of its packets carries it.
    pub(crate) tick: EncodedTick,
    /// The candidates in priority and packing order.
    pub(crate) candidates: &'a [SendCandidate],
    pub(crate) messages: &'a M,
    /// The largest payload one packet carries, after the packet session's headroom.
    pub(crate) max_payload: usize,
}

/// `PacketBuilder` packs the messages of a flush into packets.
#[derive(Debug)]
pub(crate) struct PacketBuilder {
    buffer_pool: PacketBufferPool,
}

impl Default for PacketBuilder {
    fn default() -> Self {
        Self {
            buffer_pool: PacketBufferPool::new(Self::buffer_capacity(DEFAULT_MAX_PAYLOAD)),
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
    ///
    /// Each packet holds at least its first candidate: a fragment fits a packet by the fragment
    /// size, and a whole message is at most a fragment long.
    pub(crate) fn build_next_packet<M: CandidateMessages>(
        &mut self,
        source: &PacketSource<'_, M>,
        cursor: &mut CandidateCursor,
    ) -> Option<Packet> {
        let PacketSource {
            tick,
            candidates,
            messages,
            max_payload,
        } = *source;
        let limit = OutgoingPayload::HEADROOM + max_payload;
        // Final fragments can consume singles which appear later in their priority group. Skip
        // those candidates once the main cursor reaches the single portion of the group.
        while cursor.index < cursor.single_index
            && candidates
                .get(cursor.index)
                .is_some_and(SendCandidate::is_single)
        {
            cursor.index += 1;
        }
        let first = candidates.get(cursor.index)?;
        // A fragmented message's candidate stays current while its due fragments go, one in
        // each packet.
        let key = cursor.fragment.take().unwrap_or(first.key);

        let mut open = OpenPacket {
            packet: self.new_staged_packet(PacketKind::Data, tick),
            batch: None,
            limit,
        };
        match messages.message(first.channel_id, key) {
            MessageRef::Fragment(fragment) => {
                let priority = first.rank.priority;
                open.packet.kind = PacketKind::DataFragment;
                first.channel_id.to_bytes(&mut open.packet.payload);
                fragment.write(&mut open.packet.payload);
                open.packet.record_message(PacketMessage {
                    commit: SendCommit {
                        channel_id: first.channel_id,
                        key,
                    },
                    len: BenchCounter::new(fragment.payload().len()),
                });
                debug_assert!(
                    open.packet.payload.len() <= limit,
                    "a fragment of {} bytes passes the payload of {max_payload}",
                    open.packet.len()
                );
                cursor.fragment = messages.next_due(first.channel_id, key);
                if cursor.fragment.is_none() {
                    cursor.index += 1;
                }

                // The wire format permits singles after the final fragment, which fill it. The
                // separate single cursor can look past other fragments in the same priority group,
                // letting every fragmented message use its final packet's remaining space.
                if fragment.is_last() {
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
                        if !open.try_append_single(candidate, message) {
                            break;
                        }
                        cursor.single_index += 1;
                    }
                }
            }
            MessageRef::Single(_) => {
                while let Some(candidate) = candidates.get(cursor.index) {
                    let MessageRef::Single(message) =
                        messages.message(candidate.channel_id, candidate.key)
                    else {
                        break;
                    };
                    if !open.try_append_single(candidate, message) {
                        break;
                    }
                    cursor.index += 1;
                }
                debug_assert!(
                    open.packet.payload.len() <= limit,
                    "a whole message of {} bytes passes the payload of {max_payload}",
                    open.packet.len()
                );
                cursor.single_index = cursor.single_index.max(cursor.index);
            }
        }
        Some(open.packet)
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
        }
    }
}

impl OpenPacket {
    /// Appends `message` when it fits, and returns whether it did; the first message of a packet
    /// is appended whatever its size, as a packet carries at least one.
    fn try_append_single(&mut self, candidate: &SendCandidate, message: &SingleBody) -> bool {
        let opens_batch = !self.batch.is_some_and(|state| {
            state.channel_id == candidate.channel_id && state.count < MAX_MESSAGES_PER_CHANNEL_BATCH
        });
        let append_len = if opens_batch {
            candidate.channel_id.bytes_len() + 1
        } else {
            0
        } + message.bytes_len();
        let packet = &mut self.packet;
        if !packet.messages.is_empty() && packet.payload.len() + append_len > self.limit {
            return false;
        }

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
        true
    }
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
