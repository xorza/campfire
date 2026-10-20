use crate::bench_counter::BenchCounter;
use crate::candidate_order::CandidateOrder;
use crate::channel_receiver::BufferOutcome;
use crate::channel_receiver::ChannelReceiver;
use crate::channel_sender::ChannelSender;
use crate::channel_sender::SendFlushOutcome;
use crate::channel_slots::ChannelSlots;
use crate::channel_type_id::ChannelTypeId;
use crate::compression::decompress_payload;
use crate::compression_scratch::CompressionScratch;
use crate::compression_setup::CompressionSetup;
use crate::fragment_body::FragmentBody;
use crate::message_id::MessageId;
use crate::packet::{FRAGMENT_LEN, MIN_FRAGMENT_PAYLOAD, fragment_len_for_max_payload};
use crate::packet_builder::PacketBuilder;
use crate::packet_builder::{CandidateCursor, PacketSource};
use crate::packet_error::PacketError;
use crate::packet_kind::PacketKind;
use crate::receive_budget::{DEFAULT_RECEIVE_BUDGET, ReceiveBudget};
use crate::receive_message::ReceiveMessage;
use crate::single_body::SingleBody;
use crate::tick_decoder::TickDecoder;
use crate::tick_encoder::TickEncoder;
use bytes::Bytes;
use campfire_net_bytes::{ReadInteger, Reader, SerializationError, ToBytes};
use campfire_net_core::Instant;
use campfire_net_core::{NetId, NetTick};
use campfire_net_payload::{
    OpenedPayload, PacketNumber, PacketSessionQueues, PayloadFate, PayloadKind, PayloadToken,
};
use core::time::Duration;
use tracing::{error, trace};

use crate::channel::Channel;
use crate::transport_error::TransportError;
use campfire_net_bytes::Writer;
use campfire_net_payload::SendPayload;

use crate::channel_setup::ChannelSetup;
use crate::sent_packets::{SentMessage, SentPackets};

/// A message that [`Channels::send`] staged for the next send (13-net-stack/transport.md, A queued
/// message keeps its channel's type until the flush).
#[derive(Debug)]
pub(crate) struct StagedMessage {
    pub(crate) channel_id: NetId,
    pub(crate) bytes: Bytes,
    pub(crate) priority: f32,
}

/// A staged message that a new link's packet session refused: its payload makes the channel's
/// message bound smaller than the message, so the message is dropped before it is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefusedMessage {
    pub channel: ChannelTypeId,
    pub len: usize,
    pub max_len: usize,
}

/// What became of one received packet whose bytes keep the protocol; a protocol violation is the
/// `Err` of `receive_packet`, which closes the link (13-net-stack/transport.md, A received packet
/// ends one of three ways).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PacketOutcome {
    /// The payload reached its channels, and the frame accepts the packet, which the packet session
    /// then acknowledges.
    Admitted { remote_tick: NetTick },
    /// The receive budget had no room for a message it carries. It is not accepted, so the
    /// peer finds it lost and sends its reliable messages again once deliveries free room.
    OverBudget,
}

/// Reads one packet of `kind`, as its packet session opened it, into `channels`.
///
/// The packet session dropped every copy and replay of a packet it opened before, so each packet
/// that reaches here is new.
fn receive_packet(
    channels: &mut Channels,
    kind: PayloadKind,
    packet: Bytes,
) -> Result<PacketOutcome, TransportError> {
    let packet_len = packet.len();
    channels.received_bytes.add(packet_len);
    let mut packet_kind = PacketKind::try_from(kind)?;
    let mut cursor = Reader::from(packet);
    let tick = channels.tick_decoder.read(&mut cursor)?;
    trace!(
        target: "campfire_net_debug::transport",
        kind = "packet_recv",
        schedule = "PreUpdate",
        sample_point = "PreUpdate",
        remote_tick = tick.0,
        bytes = packet_len,
        ?packet_kind,
        "received transport packet"
    );

    let mut budget = channels.receive_room();
    if packet_kind.is_compressed() {
        let compressed_payload = cursor.take_remaining();
        decompress_payload(
            compressed_payload.as_ref(),
            channels.compression,
            &mut channels.decompression,
        )?;
        cursor = Reader::from(channels.decompression.take_written());
        packet_kind = packet_kind.uncompressed_variant();
    }

    if packet_kind == PacketKind::DataFragment {
        let channel_id = NetId::from_bytes(&mut cursor)?;
        let fragment_body = FragmentBody::from_bytes(&mut cursor)?;
        trace!(
            target: "campfire_net_debug::transport",
            kind = "channel_recv_fragment",
            schedule = "PreUpdate",
            sample_point = "PreUpdate",
            remote_tick = tick.0,
            channel_id = channel_id.get(),
            bytes = fragment_body.bytes.len(),
            "received channel fragment"
        );
        let compression = channels.compression;
        let receiver = channels
            .receivers
            .get_mut(channel_id)
            .ok_or(PacketError::ChannelNotFound)?;
        receiver.received_bytes.add(fragment_body.bytes.len());
        let message = ReceiveMessage {
            data: fragment_body.into(),
            remote_sent_tick: tick,
            compression,
        };
        if receiver.buffer_receive(message, &mut budget)? == BufferOutcome::OverBudget {
            return Ok(PacketOutcome::OverBudget);
        }
    }
    while cursor.has_remaining() {
        let channel_id = NetId::from_bytes(&mut cursor)?;
        let messages = cursor.read_u8().map_err(SerializationError::from)?;
        trace!(
            target: "campfire_net_debug::transport",
            kind = "channel_recv_batch",
            schedule = "PreUpdate",
            sample_point = "PreUpdate",
            remote_tick = tick.0,
            channel_id = channel_id.get(),
            messages,
            "received channel message batch"
        );
        let compression = channels.compression;
        let receiver = channels
            .receivers
            .get_mut(channel_id)
            .ok_or(PacketError::ChannelNotFound)?;
        let has_id = receiver.single_has_id();
        for _ in 0..messages {
            let single_body = SingleBody::read(&mut cursor, has_id)?;
            trace!(
                target: "campfire_net_debug::transport",
                kind = "channel_recv_message",
                schedule = "PreUpdate",
                sample_point = "PreUpdate",
                remote_tick = tick.0,
                channel_id = channel_id.get(),
                bytes = single_body.bytes.len(),
                "received channel message bytes"
            );
            receiver.received_bytes.add(single_body.bytes.len());
            let message = ReceiveMessage {
                data: single_body.into(),
                remote_sent_tick: tick,
                compression,
            };
            if receiver.buffer_receive(message, &mut budget)? == BufferOutcome::OverBudget {
                return Ok(PacketOutcome::OverBudget);
            }
        }
    }
    Ok(PacketOutcome::Admitted { remote_tick: tick })
}

/// The channels of one link and their two packet steps: [`Self::receive`] reads what the link's
/// packet session opened into the channels, and [`Self::flush`] packs the channels' messages into
/// the packet session's budget.
#[derive(Debug)]
pub struct Channels {
    pub(crate) receivers: ChannelSlots<ChannelReceiver>,
    pub(crate) senders: ChannelSlots<ChannelSender>,
    /// The order of the due messages of every channel of this transport.
    pub(crate) candidate_order: CandidateOrder,
    /// PacketBuilder shared between all channels of this transport
    pub(crate) packet_builder: PacketBuilder,
    /// The token of the next packet the transport sends, which names it in its fate.
    next_token: u64,
    /// Writes each packet's tick, short once the peer surely reads it whole.
    pub(crate) tick_encoder: TickEncoder,
    /// Reads each received packet's tick.
    pub(crate) tick_decoder: TickDecoder,
    /// The packets of the frame's receive that reached their channels, which the packet session
    /// acknowledges.
    pub(crate) accepted: Vec<PacketNumber>,
    /// The longest the transport stays silent: past it, a flush with no message sends a packet
    /// of its tick alone.
    heartbeat: Duration,
    /// When the transport last gave its packet session a packet.
    pub(crate) last_sent: Option<Instant>,
    /// Compression workspace shared by all channel senders and the packet builder.
    pub(crate) compression_scratch: CompressionScratch,
    /// The chunks that received packets decompress into: a message read from one keeps its
    /// chunk until the message is dropped.
    pub(crate) decompression: Writer,
    /// Fragment payload size of the senders, derived from the largest payload of the link's
    /// packet session.
    fragment_len: usize,
    compression: CompressionSetup,
    /// The bytes the receive side may hold for messages it can not deliver yet; see
    /// [`Self::with_receive_budget`].
    receive_budget: usize,

    /// The packets in flight: the tick of each, and its reliable messages, so that a sender learns
    /// each one's fate. The packet session reports each packet's fate once, so an entry lives until
    /// its packet is delivered or lost.
    pub(crate) sent: SentPackets,
    /// Messages staged for the next send, in staging order (13-net-stack/transport.md, A link's
    /// `Channels` queue its sends in a `Vec`, in the `Transport` component).
    pub(crate) staged: Vec<StagedMessage>,
    /// Bytes of the packets given to the packet session.
    pub(crate) sent_bytes: BenchCounter,
    /// Packets given to the packet session.
    pub(crate) sent_packets: BenchCounter,
    /// Messages and fragments that a packet given to the packet session carried again.
    pub(crate) resent_messages: BenchCounter,
    /// Bytes of the packets the packet session opened.
    pub(crate) received_bytes: BenchCounter,
    /// Bytes that compression removed from the packets sent.
    pub(crate) compression_saved_bytes: BenchCounter,
    /// Packets sent uncompressed as their compressed form was not smaller.
    pub(crate) compression_abandoned: BenchCounter,
}

impl Default for Channels {
    fn default() -> Self {
        Self {
            receivers: ChannelSlots::default(),
            senders: ChannelSlots::default(),
            candidate_order: CandidateOrder::default(),
            packet_builder: PacketBuilder::default(),
            next_token: 0,
            tick_encoder: TickEncoder::default(),
            tick_decoder: TickDecoder::default(),
            accepted: Vec::new(),
            heartbeat: Self::DEFAULT_HEARTBEAT,
            last_sent: None,
            compression_scratch: CompressionScratch::default(),
            decompression: Writer::default(),
            fragment_len: FRAGMENT_LEN,
            compression: CompressionSetup::default(),
            receive_budget: DEFAULT_RECEIVE_BUDGET,
            sent: SentPackets::default(),
            staged: Vec::new(),
            sent_bytes: BenchCounter::default(),
            sent_packets: BenchCounter::default(),
            resent_messages: BenchCounter::default(),
            received_bytes: BenchCounter::default(),
            compression_saved_bytes: BenchCounter::default(),
            compression_abandoned: BenchCounter::default(),
        }
    }
}

impl Channels {
    /// The default [`Self::set_heartbeat`]: 100 ms, the interval the ping messages kept before
    /// the packet session gave the RTT.
    pub const DEFAULT_HEARTBEAT: Duration = Duration::from_millis(100);

    /// Sets how the packets and fragmented messages this transport sends are compressed, and the
    /// limits it decompresses what it receives within.
    pub const fn set_compression(&mut self, compression: CompressionSetup) {
        self.compression = compression;
    }

    /// Sets the bytes the receive side may hold for messages it can not deliver yet: partial
    /// messages being reassembled, and ordered messages waiting behind a gap. A packet that would
    /// pass it is not accepted, so the packet session sends no ACK for it and the peer sends its
    /// reliable messages again once it finds the packet lost.
    ///
    /// It must be at least twice every receiving channel's `max_message_len`, the most one
    /// partial message can reserve, so that the largest message always completes.
    #[must_use]
    pub fn with_receive_budget(mut self, bytes: usize) -> Self {
        self.receive_budget = bytes;
        for receiver in &self.receivers {
            self.assert_budget_admits(receiver.max_message_len());
        }
        self
    }

    fn assert_budget_admits(&self, max_message_len: usize) {
        assert!(
            self.receive_budget >= max_message_len.saturating_mul(2),
            "a receive budget of {} bytes can not admit a channel's {max_message_len}-byte \
             message: it needs twice that",
            self.receive_budget
        );
    }

    /// The room left in the receive budget once every channel's held bytes are counted.
    pub(crate) fn receive_room(&self) -> ReceiveBudget {
        let held = self
            .receivers
            .iter()
            .map(ChannelReceiver::held)
            .sum::<usize>();
        ReceiveBudget::new(self.receive_budget.saturating_sub(held))
    }

    /// Sets the longest the transport stays silent; see [`Self::set_heartbeat`].
    #[must_use]
    pub const fn with_heartbeat(mut self, heartbeat: Duration) -> Self {
        self.heartbeat = heartbeat;
        self
    }

    /// Sets the longest the transport stays silent: a flush with no message that comes `heartbeat`
    /// or more after the last packet sends a packet of its tick alone. The peer's timeline reads
    /// the tick of each packet, and the link's RTT samples come from its ACK, so an idle peer would
    /// give neither (13-net-stack/transport.md, An idle transport sends its tick).
    pub const fn set_heartbeat(&mut self, heartbeat: Duration) {
        self.heartbeat = heartbeat;
    }

    /// Whether a flush at `now` that sent nothing sends a packet of its tick alone.
    pub(crate) fn is_heartbeat_due(&self, now: Instant) -> bool {
        self.last_sent
            .is_none_or(|last| now.saturating_duration_since(last) >= self.heartbeat)
    }

    /// The token of the next packet sent.
    pub(crate) const fn next_token(&mut self) -> PayloadToken {
        let token = PayloadToken(self.next_token);
        self.next_token += 1;
        token
    }
}

impl Channels {
    /// A new packet session: its tick has no reference yet, and its largest payload, once it handed
    /// one over, sizes the fragments. A smaller fragment makes a channel's message bound smaller,
    /// so each staged message above its new bound is dropped and returned.
    pub fn link(&mut self, max_payload: usize) -> Result<Vec<RefusedMessage>, PacketError> {
        self.tick_encoder.reset();
        self.tick_decoder.reset();
        if max_payload > 0 {
            self.size_fragments(max_payload)?;
        }
        let mut refused = Vec::new();
        let Self {
            staged,
            senders,
            fragment_len,
            ..
        } = self;
        staged.retain(|message| {
            let max_len = senders[message.channel_id].max_message_len(*fragment_len);
            let fits = message.bytes.len() <= max_len;
            if !fits {
                refused.push(RefusedMessage {
                    channel: senders[message.channel_id].channel_type_id(),
                    len: message.bytes.len(),
                    max_len,
                });
            }
            fits
        });
        Ok(refused)
    }

    /// The size the senders split a message's fragments to, which the largest payload of the
    /// link's packet session sets at each link.
    pub const fn fragment_len(&self) -> usize {
        self.fragment_len
    }

    /// Derives the senders' fragment size from the largest payload of the link's packet session.
    pub fn size_fragments(&mut self, max_payload: usize) -> Result<(), PacketError> {
        let fragment_len =
            fragment_len_for_max_payload(max_payload).ok_or(PacketError::PayloadTooSmall {
                actual: max_payload,
                min: MIN_FRAGMENT_PAYLOAD,
            })?;
        self.fragment_len = fragment_len;
        Ok(())
    }

    /// The receiving channel registered for `channel_id`.
    pub fn channel_receiver_mut(&mut self, channel_id: NetId) -> Option<&mut ChannelReceiver> {
        self.receivers.get_mut(channel_id)
    }

    /// Iterates over the registered receiving channels.
    pub fn channel_receivers(&self) -> impl Iterator<Item = &ChannelReceiver> {
        self.receivers.iter()
    }

    /// Mutably iterates over the registered receiving channels.
    pub fn channel_receivers_mut(&mut self) -> impl Iterator<Item = &mut ChannelReceiver> {
        self.receivers.iter_mut()
    }

    /// Adds a manually configured sending channel.
    ///
    /// An app's setup adds its channels from the channel registry, as `Transport` does.
    pub fn add_channel_send<C: Channel>(&mut self, setup: ChannelSetup, channel_id: NetId) {
        self.senders.insert(
            channel_id,
            ChannelSender::new(
                ChannelTypeId::of::<C>(),
                channel_id,
                core::any::type_name::<C>(),
                &setup,
            ),
        );
    }

    /// Adds a manually configured receiving channel.
    ///
    /// An app's setup adds its channels from the channel registry, as `Transport` does.
    pub fn add_channel_receive<C: Channel>(&mut self, setup: ChannelSetup, channel_id: NetId) {
        self.assert_budget_admits(setup.max_message_len);
        self.receivers.insert(
            channel_id,
            ChannelReceiver::new(
                ChannelTypeId::of::<C>(),
                core::any::type_name::<C>(),
                &setup,
            ),
        );
    }

    /// Stages a message on the sending channel `channel_id` for the next send.
    ///
    /// A message above the channel's `max_message_len` is refused with
    /// [`TransportError::MessageTooLarge`]; nothing is staged then.
    pub fn send(
        &mut self,
        channel_id: NetId,
        bytes: SendPayload,
        priority: f32,
    ) -> Result<(), TransportError> {
        self.check_message_len(channel_id, bytes.len())?;
        self.staged.push(StagedMessage {
            channel_id,
            bytes,
            priority,
        });
        Ok(())
    }

    fn check_message_len(&self, channel_id: NetId, len: usize) -> Result<(), TransportError> {
        let max_len = self
            .senders
            .get(channel_id)
            .ok_or(TransportError::ChannelNotFound(channel_id))?
            .max_message_len(self.fragment_len);
        if len > max_len {
            return Err(TransportError::MessageTooLarge { len, max_len });
        }
        Ok(())
    }

    /// Buffers bytes in the sender of `channel_id`, a channel this transport sends on, of a size
    /// its bound was checked against: at staging, and again at each link that sizes the bound.
    fn buffer_send(
        &mut self,
        channel_id: NetId,
        bytes: SendPayload,
        priority: f32,
    ) -> Option<MessageId> {
        debug_assert!(
            self.check_message_len(channel_id, bytes.len()).is_ok(),
            "a message is checked against its bound before it is buffered"
        );
        let Self {
            senders,
            compression_scratch,
            compression,
            fragment_len,
            ..
        } = self;
        let channel_sender = senders
            .get_mut(channel_id)
            .expect("a channel id names a sender of this transport");
        channel_sender.buffer_send(
            bytes,
            priority,
            *fragment_len,
            *compression,
            compression_scratch,
        )
    }

    /// The receive step at `now`, after a frame of `delta`: the channels' timers advance, each
    /// packet the packet session opened reaches its channels, the packet session accepts each one
    /// that did, and each sender learns the fate of each packet that carried its reliable messages.
    /// `admitted` hears the remote tick of each packet that reached its channels. A packet that
    /// breaks the protocol ends the reading, and its error comes back once the fates are taken.
    pub fn receive(
        &mut self,
        queues: &mut PacketSessionQueues,
        now: Instant,
        delta: Duration,
        mut admitted: impl FnMut(NetTick),
    ) -> Result<(), TransportError> {
        self.senders
            .iter_mut()
            .for_each(|channel_sender| channel_sender.update(delta));
        self.receivers
            .iter_mut()
            .for_each(|channel_receiver| channel_receiver.update(now));
        let mut violation = None;
        for OpenedPayload {
            number,
            kind,
            payload,
        } in queues.opened()
        {
            // The packet session is the last receive stage that changes a payload in place. Freeze
            // here so transport parsing can retain cheap immutable subslices.
            match receive_packet(self, kind, payload.freeze()) {
                Ok(PacketOutcome::Admitted { remote_tick }) => {
                    self.accepted.push(number);
                    admitted(remote_tick);
                }
                Ok(PacketOutcome::OverBudget) => {
                    trace!("refused a packet over the receive budget");
                }
                Err(error) => {
                    violation = Some(error);
                    break;
                }
            }
        }
        for number in self.accepted.drain(..) {
            queues.accept(number);
        }

        let Self {
            sent,
            senders,
            tick_encoder,
            ..
        } = self;
        for fate in queues.fates() {
            let (token, delivered) = match fate {
                PayloadFate::Delivered(token) => (token, true),
                PayloadFate::Lost(token) => (token, false),
            };
            trace!(
                target: "campfire_net_debug::transport",
                kind = "packet_fate",
                schedule = "PreUpdate",
                sample_point = "PreUpdate",
                ?token,
                delivered,
                "transport packet fate"
            );
            let Some(packet) = sent.take(token) else {
                continue;
            };
            if delivered {
                tick_encoder.on_delivered(packet.tick);
            }
            for SentMessage { channel_id, key } in sent.messages(&packet) {
                let channel_sender = senders.get_mut(channel_id);
                debug_assert!(
                    channel_sender.is_some(),
                    "a fate is tracked only for a message a sender of this transport sent"
                );
                let Some(channel_sender) = channel_sender else {
                    continue;
                };
                if !delivered {
                    channel_sender.receive_loss(key);
                } else if channel_sender.receive_ack(key) {
                    trace!(
                        "Acked message: channel={:?},message_ack={:?}",
                        channel_sender.name(),
                        key
                    );
                }
            }
        }
        sent.release();
        violation.map_or(Ok(()), Err)
    }

    /// The send step at `now` for the tick `tick`: each staged message enters its channel, and
    /// packets go to the packet session while its budget lasts; a flush that sends nothing past the
    /// heartbeat sends the tick alone. A packet session that handed over nothing yet takes nothing.
    pub fn flush(&mut self, queues: &mut PacketSessionQueues, tick: NetTick, now: Instant) {
        // A link whose packet session has not handed over yet takes nothing: the staged messages
        // wait for the next frame.
        let max_payload = queues.max_payload();
        if max_payload == 0 {
            return;
        }
        self.packet_builder.begin_send(max_payload);
        // Taken out while each message enters its channel, and put back with its capacity.
        let mut staged = core::mem::take(&mut self.staged);
        #[expect(clippy::iter_with_drain, reason = "the drained list is put back")]
        for StagedMessage {
            channel_id,
            bytes,
            priority,
        } in staged.drain(..)
        {
            trace!(
                target: "campfire_net_debug::transport",
                kind = "channel_send_buffer",
                schedule = "PostUpdate",
                sample_point = "PostUpdate",
                tick = ?tick,
                tick_id = u64::from(tick.0),
                channel = %self.senders[channel_id].name(),
                bytes = bytes.len(),
                priority = priority,
                "buffered channel message for transport send"
            );
            self.buffer_send(channel_id, bytes, priority);
        }
        self.staged = staged;

        // Collect candidates while each channel keeps its pending queues.
        self.candidate_order.clear();
        {
            let candidates = self.candidate_order.candidates_mut();
            self.senders.iter_mut().for_each(|channel| {
                channel.collect_send_candidates(candidates);
            });
        }
        self.candidate_order.prioritize();

        let encoded_tick = self.tick_encoder.encode(tick);
        let mut candidate_cursor = CandidateCursor::default();
        let mut total_bytes_sent: usize = 0;
        let mut first_token = None;
        let mut flush_outcome = SendFlushOutcome::Complete;
        loop {
            let source = PacketSource {
                tick: encoded_tick,
                candidates: self.candidate_order.candidates(),
                messages: &self.senders,
                compression: self.compression,
                max_payload,
            };
            let staged = self.packet_builder.build_next_packet(
                &source,
                &mut candidate_cursor,
                &mut self.compression_scratch,
            );
            let mut packet = match staged {
                Ok(Some(packet)) => packet,
                Ok(None) => break,
                Err(error) => {
                    error!(?error, "failed to stage transport packet");
                    break;
                }
            };
            let messages = packet.messages.len();
            let packet_len = packet.len();
            let packet_compression = packet.compression;
            if packet_len > queues.budget() {
                // Staging has no channel side effects. Reliable messages are retained; each
                // unreliable channel applies its retry-unsent policy when this flush
                // finishes. Later candidates are not higher priority than this packet.
                flush_outcome = SendFlushOutcome::BudgetLimited;
                self.packet_builder.recycle_packet(packet);
                break;
            }
            let token = self.next_token();
            first_token.get_or_insert(token);
            trace!(
                target: "campfire_net_debug::transport",
                kind = "packet_send",
                schedule = "PostUpdate",
                sample_point = "PostUpdate",
                ?token,
                local_tick = tick.0,
                bytes = packet_len,
                messages,
                compression_enabled = self.compression.is_enabled(),
                compression_algorithm = ?self.compression.algorithm,
                packet_compressed = packet_compression.is_some(),
                compression_original_len = packet_compression.map_or(0, |info| info.original_len),
                compression_compressed_len = packet_compression.map_or(0, |info| info.compressed_len),
                "sending transport packet"
            );

            // The packet session's queue is the transactional boundary: the channels' send state,
            // the fate tracking and the bench counters change only past this point.
            total_bytes_sent += packet_len;
            let kind = packet.kind;
            let payload = packet.take_send_payload();
            queues.send(token, kind.into(), payload);
            self.sent.push(token, tick);
            self.sent_packets.add(1);
            if let Some(compression_info) = packet.compression {
                self.compression_saved_bytes
                    .add(compression_info.original_len - compression_info.compressed_len);
            }
            if packet.compression_abandoned {
                self.compression_abandoned.add(1);
            }

            let mut packet_messages = core::mem::take(&mut packet.messages);
            #[expect(clippy::iter_with_drain, reason = "the drained list is recycled")]
            for packet_message in packet_messages.drain(..) {
                let commit = packet_message.commit;
                let channel_sender = self
                    .senders
                    .get_mut(commit.channel_id)
                    .expect("staged candidate channel must remain registered during flush");
                if channel_sender.commit_send(commit.key) {
                    self.resent_messages.add(1);
                }
                channel_sender.sent_bytes += packet_message.len;
                if channel_sender.watches_acks() {
                    self.sent.push_message(SentMessage {
                        channel_id: commit.channel_id,
                        key: commit.key,
                    });
                }
            }
            self.packet_builder.recycle_packet_messages(packet_messages);
        }
        if first_token.is_none() && self.is_heartbeat_due(now) {
            let mut packet = self.packet_builder.build_tick_packet(encoded_tick);
            if packet.len() <= queues.budget() {
                let token = self.next_token();
                first_token = Some(token);
                total_bytes_sent += packet.len();
                let payload = packet.take_send_payload();
                queues.send(token, packet.kind.into(), payload);
                self.sent.push(token, tick);
                self.sent_packets.add(1);
                trace!(
                    ?token,
                    local_tick = tick.0,
                    "sending a heartbeat of the tick"
                );
                self.packet_builder
                    .recycle_packet_messages(core::mem::take(&mut packet.messages));
            } else {
                self.packet_builder.recycle_packet(packet);
            }
        }
        if first_token.is_some() {
            self.tick_encoder.on_sent(tick);
            self.last_sent = Some(now);
        }
        self.senders
            .iter_mut()
            .for_each(|channel| channel.end_send(flush_outcome));
        self.candidate_order.clear();
        if total_bytes_sent > 0 {
            trace!(
                target: "campfire_net_debug::transport",
                kind = "send_flush",
                schedule = "PostUpdate",
                sample_point = "PostUpdate",
                local_tick = tick.0,
                sent_bytes = total_bytes_sent,
                "flushed transport packets to the packet session"
            );
        }
        self.sent_bytes.add(total_bytes_sent);
    }

    /// Resets the channels to their first state, as a link that disconnected needs.
    pub fn reset(&mut self) {
        self.receivers.iter_mut().for_each(ChannelReceiver::reset);
        self.senders.iter_mut().for_each(ChannelSender::reset);
        self.candidate_order = CandidateOrder::default();
        self.packet_builder = PacketBuilder::default();
        self.tick_encoder.reset();
        self.tick_decoder.reset();
        self.accepted.clear();
        self.last_sent = None;
        self.compression_scratch = CompressionScratch::default();
        self.sent.clear();
        self.staged.clear();
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::channel_receiver::ChannelReceiver;
    use crate::channel_sender::ChannelSender;
    use crate::channel_type_id::ChannelTypeId;
    use crate::channels::Channels;
    use crate::message_id::MessageId;
    use crate::transport_error::TransportError;
    use campfire_net_core::NetId;
    use campfire_net_payload::SendPayload;
    use core::ops::{Add, Sub};

    /// A reading of one channel's bench counters on a [`Channels`].
    ///
    /// The byte fields count message payload bytes (for a fragment, the fragment's bytes); every
    /// field accumulates over the channel's life, so a bench subtracts two readings.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct ChannelCounts {
        /// Bytes of the channel's messages in packets given to the packet session.
        pub sent_bytes: u64,
        /// The channel's messages and fragments in packets given to the packet session.
        pub sent_messages: u64,
        /// Bytes of the channel's messages parsed from received packets.
        pub received_bytes: u64,
    }

    impl Add for ChannelCounts {
        type Output = Self;

        fn add(self, rhs: Self) -> Self {
            Self {
                sent_bytes: self.sent_bytes + rhs.sent_bytes,
                sent_messages: self.sent_messages + rhs.sent_messages,
                received_bytes: self.received_bytes + rhs.received_bytes,
            }
        }
    }

    impl Sub for ChannelCounts {
        type Output = Self;

        fn sub(self, base: Self) -> Self {
            Self {
                sent_bytes: self.sent_bytes - base.sent_bytes,
                sent_messages: self.sent_messages - base.sent_messages,
                received_bytes: self.received_bytes - base.received_bytes,
            }
        }
    }

    /// What a [`Channels`] still waits for.
    ///
    /// Once a healed network has delivered everything owed, no fragmented message is partial and
    /// only the last frame's packets wait for their fates.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct TransportBacklog {
        /// Fragmented messages with some fragments still missing, over every channel.
        pub partial_fragments: usize,
        /// Packets whose reliable messages wait for the packet's fate.
        pub tracked_packet_acks: usize,
    }

    /// A reading of a link's channels' bench counters.
    ///
    /// Every field is accumulated over the channels' life, so a bench subtracts two readings.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct TransportCounts {
        /// Bytes of the packets given to the packet session.
        pub sent_bytes: u64,
        /// Packets given to the packet session.
        pub sent_packets: u64,
        /// Messages and fragments that a packet given to the packet session carried again.
        pub resent_messages: u64,
        /// Bytes of the packets the packet session opened.
        pub received_bytes: u64,
        /// Bytes that compression removed from the packets sent.
        pub compression_saved_bytes: u64,
        /// Packets whose compressed form was not smaller, so they were sent uncompressed.
        pub compression_abandoned: u64,
    }

    impl Add for TransportCounts {
        type Output = Self;

        fn add(self, rhs: Self) -> Self {
            Self {
                sent_bytes: self.sent_bytes + rhs.sent_bytes,
                sent_packets: self.sent_packets + rhs.sent_packets,
                resent_messages: self.resent_messages + rhs.resent_messages,
                received_bytes: self.received_bytes + rhs.received_bytes,
                compression_saved_bytes: self.compression_saved_bytes + rhs.compression_saved_bytes,
                compression_abandoned: self.compression_abandoned + rhs.compression_abandoned,
            }
        }
    }

    impl Sub for TransportCounts {
        type Output = Self;

        fn sub(self, base: Self) -> Self {
            Self {
                sent_bytes: self.sent_bytes - base.sent_bytes,
                sent_packets: self.sent_packets - base.sent_packets,
                resent_messages: self.resent_messages - base.resent_messages,
                received_bytes: self.received_bytes - base.received_bytes,
                compression_saved_bytes: self.compression_saved_bytes
                    - base.compression_saved_bytes,
                compression_abandoned: self.compression_abandoned - base.compression_abandoned,
            }
        }
    }

    impl Channels {
        /// Buffers bytes directly in the channel sender, as the next send does with each staged
        /// message.
        pub fn send_mut(
            &mut self,
            channel_id: NetId,
            bytes: SendPayload,
            priority: f32,
        ) -> Result<Option<MessageId>, TransportError> {
            self.check_message_len(channel_id, bytes.len())?;
            Ok(self.buffer_send(channel_id, bytes, priority))
        }

        /// The sending channel `channel`.
        pub(crate) fn sender(&self, channel: ChannelTypeId) -> Option<&ChannelSender> {
            self.senders
                .iter()
                .find(|sender| sender.channel_type_id() == channel)
        }

        /// Each reliable message not acknowledged yet, by its channel.
        pub fn pending_reliable(&self) -> Vec<(ChannelTypeId, MessageId)> {
            self.senders
                .iter()
                .flat_map(|sender| {
                    let channel = sender.channel_type_id();
                    sender
                        .pending_reliable()
                        .into_iter()
                        .map(move |message| (channel, message))
                })
                .collect()
        }

        /// What the transport still waits for.
        pub fn backlog(&self) -> TransportBacklog {
            TransportBacklog {
                partial_fragments: self
                    .receivers
                    .iter()
                    .map(ChannelReceiver::partial_fragments)
                    .sum(),
                tracked_packet_acks: self.sent.with_messages(),
            }
        }

        /// The transport's bench counters, accumulated over its life.
        pub const fn counts(&self) -> TransportCounts {
            TransportCounts {
                sent_bytes: self.sent_bytes.get(),
                sent_packets: self.sent_packets.get(),
                resent_messages: self.resent_messages.get(),
                received_bytes: self.received_bytes.get(),
                compression_saved_bytes: self.compression_saved_bytes.get(),
                compression_abandoned: self.compression_abandoned.get(),
            }
        }

        /// The bench counters of `channel`, zero when the transport lacks a side of it.
        pub fn channel_counts(&self, channel: ChannelTypeId) -> ChannelCounts {
            ChannelCounts {
                sent_bytes: self
                    .sender(channel)
                    .map_or(0, |sender| sender.sent_bytes.get()),
                sent_messages: self
                    .sender(channel)
                    .map_or(0, |sender| sender.sent_messages.get()),
                received_bytes: self
                    .receivers
                    .iter()
                    .find(|receiver| receiver.channel_type_id() == channel)
                    .map_or(0, |receiver| receiver.received_bytes.get()),
            }
        }
    }
}

#[cfg(test)]
mod tests;
