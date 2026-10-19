use crate::bench_counter::BenchCounter;
use crate::channel_kind::ChannelKind;
use crate::channel_receiver::ChannelReceiver;
use crate::channel_receiver::Receipt;
use crate::channel_sender::ChannelSender;
use crate::channel_sender::SendFlushOutcome;
use crate::channel_slots::ChannelSlots;
use crate::compression::decompress_payload;
use crate::compression_config::CompressionConfig;
use crate::compression_scratch::CompressionScratch;
use crate::fragment_data::FragmentData;
use crate::message_id::MessageId;
use crate::packet::{FRAGMENT_SIZE, MIN_FRAGMENT_PAYLOAD, fragment_size_for_max_payload};
use crate::packet_builder::PacketBuilder;
use crate::packet_builder::{CandidateCursor, PacketSource};
use crate::packet_error::PacketError;
use crate::packet_kind::PacketKind;
use crate::priority_manager::PriorityManager;
use crate::receive_budget::{DEFAULT_RECEIVE_BUDGET, ReceiveBudget};
use crate::receive_message::ReceiveMessage;
use crate::single_data::SingleData;
use crate::tick_decoder::TickDecoder;
use crate::tick_encoder::TickEncoder;
use bytes::Bytes;
use campfire_net_bytes::{ReadInteger, Reader, SerializationError, ToBytes};
use campfire_net_core::Instant;
use campfire_net_core::{NetId, NetTick};
use campfire_net_payload::{
    OpenedPayload, PacketNumber, PayloadFate, PayloadKind, PayloadToken, SessionQueues,
};
use core::time::Duration;
use tracing::{error, trace};

use crate::channel::Channel;
use crate::transport_error::TransportError;
use campfire_net_bytes::Writer;
use campfire_net_payload::SendPayload;

use crate::channel_config::ChannelConfig;
use crate::sent_packets::{SentMessage, SentPackets};

/// A message that [`Channels::send`] staged for the next send.
#[derive(Debug)]
pub(crate) struct StagedMessage {
    pub(crate) channel_id: NetId,
    pub(crate) bytes: Bytes,
    pub(crate) priority: f32,
}

/// A staged message that a new link's session refused: its payload makes the channel's message
/// bound smaller than the message, so the message is dropped before it is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefusedMessage {
    pub kind: ChannelKind,
    pub size: usize,
    pub max: usize,
}

/// What became of one received packet whose bytes keep the protocol; a protocol violation is the
/// `Err` of `receive_packet`, which closes the link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PacketOutcome {
    /// The payload reached its channels, and the frame accepts the packet, which the session
    /// then acknowledges.
    Admitted { remote_tick: NetTick },
    /// The receive budget had no room for a message it carries. It is not accepted, so the
    /// peer finds it lost and sends its reliable messages again once deliveries free room.
    OverBudget,
}

/// Reads one packet of `kind`, as its session opened it, into `channels`.
///
/// The session dropped every copy and replay of a packet it opened before, so each packet that
/// reaches here is new.
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
        let fragment_data = FragmentData::from_bytes(&mut cursor)?;
        trace!(
            target: "campfire_net_debug::transport",
            kind = "channel_recv_fragment",
            schedule = "PreUpdate",
            sample_point = "PreUpdate",
            remote_tick = tick.0,
            channel_id = channel_id.get(),
            bytes = fragment_data.bytes.len(),
            "received channel fragment"
        );
        let compression = channels.compression;
        let receiver = channels
            .receivers
            .get_mut(channel_id)
            .ok_or(PacketError::ChannelNotFound)?;
        receiver.received_bytes.add(fragment_data.bytes.len());
        let message = ReceiveMessage {
            data: fragment_data.into(),
            remote_sent_tick: tick,
            compression,
        };
        if receiver.buffer_receive(message, &mut budget)? == Receipt::OverBudget {
            return Ok(PacketOutcome::OverBudget);
        }
    }
    while cursor.has_remaining() {
        let channel_id = NetId::from_bytes(&mut cursor)?;
        let num_messages = cursor.read_u8().map_err(SerializationError::from)?;
        trace!(
            target: "campfire_net_debug::transport",
            kind = "channel_recv_batch",
            schedule = "PreUpdate",
            sample_point = "PreUpdate",
            remote_tick = tick.0,
            channel_id = channel_id.get(),
            num_messages,
            "received channel message batch"
        );
        let compression = channels.compression;
        let receiver = channels
            .receivers
            .get_mut(channel_id)
            .ok_or(PacketError::ChannelNotFound)?;
        let has_id = receiver.single_has_id();
        for _ in 0..num_messages {
            let single_data = SingleData::read(&mut cursor, has_id)?;
            trace!(
                target: "campfire_net_debug::transport",
                kind = "channel_recv_message",
                schedule = "PreUpdate",
                sample_point = "PreUpdate",
                remote_tick = tick.0,
                channel_id = channel_id.get(),
                bytes = single_data.bytes.len(),
                "received channel message bytes"
            );
            receiver.received_bytes.add(single_data.bytes.len());
            let message = ReceiveMessage {
                data: single_data.into(),
                remote_sent_tick: tick,
                compression,
            };
            if receiver.buffer_receive(message, &mut budget)? == Receipt::OverBudget {
                return Ok(PacketOutcome::OverBudget);
            }
        }
    }
    Ok(PacketOutcome::Admitted { remote_tick: tick })
}

/// The channels of one link and their two packet steps: [`Self::receive`] reads what the link's
/// session opened into the channels, and [`Self::flush`] packs the channels' messages into the
/// session's budget.
#[derive(Debug)]
pub struct Channels {
    pub(crate) receivers: ChannelSlots<ChannelReceiver>,
    pub(crate) senders: ChannelSlots<ChannelSender>,
    /// PriorityManager shared between all channels of this transport
    pub(crate) priority_manager: PriorityManager,
    /// PacketBuilder shared between all channels of this transport
    pub(crate) packet_manager: PacketBuilder,
    /// The token of the next packet the transport sends, which names it in its fate.
    next_token: u64,
    /// Writes each packet's tick, short once the peer surely reads it whole.
    pub(crate) tick_encoder: TickEncoder,
    /// Reads each received packet's tick.
    pub(crate) tick_decoder: TickDecoder,
    /// The packets of the frame's receive that reached their channels, which the session
    /// acknowledges.
    pub(crate) accepted: Vec<PacketNumber>,
    /// The longest the transport stays silent: past it, a flush with no message sends a packet
    /// of its tick alone.
    heartbeat: Duration,
    /// When the transport last gave its session a packet.
    pub(crate) last_sent: Option<Instant>,
    /// Compression workspace shared by all channel senders and the packet builder.
    pub(crate) compression_scratch: CompressionScratch,
    /// The chunks that received packets decompress into: a message read from one keeps its
    /// chunk until the message is dropped.
    pub(crate) decompression: Writer,
    /// Fragment payload size of the senders, derived from the largest payload of the link's
    /// session.
    fragment_size: usize,
    compression: CompressionConfig,
    /// The bytes the receive side may hold for messages it can not deliver yet; see
    /// [`Self::with_receive_budget`].
    receive_budget: usize,

    /// The packets in flight: the tick of each, and its reliable messages, so that a sender
    /// learns each one's fate. The session reports each packet's fate once, so an entry lives
    /// until its packet is delivered or lost.
    pub(crate) sent: SentPackets,
    /// Messages staged for the next send, in staging order.
    pub(crate) staged: Vec<StagedMessage>,
    /// Bytes of the packets given to the session.
    pub(crate) sent_bytes: BenchCounter,
    /// Packets given to the session.
    pub(crate) sent_packets: BenchCounter,
    /// Messages and fragments that a packet given to the session carried again.
    pub(crate) resent_messages: BenchCounter,
    /// Bytes of the packets the session opened.
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
            priority_manager: PriorityManager::default(),
            packet_manager: PacketBuilder::default(),
            next_token: 0,
            tick_encoder: TickEncoder::default(),
            tick_decoder: TickDecoder::default(),
            accepted: Vec::new(),
            heartbeat: Self::DEFAULT_HEARTBEAT,
            last_sent: None,
            compression_scratch: CompressionScratch::default(),
            decompression: Writer::default(),
            fragment_size: FRAGMENT_SIZE,
            compression: CompressionConfig::default(),
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
    /// the session gave the RTT.
    pub const DEFAULT_HEARTBEAT: Duration = Duration::from_millis(100);

    /// Sets how the packets and fragmented messages this transport sends are compressed, and the
    /// limits it decompresses what it receives within.
    pub const fn set_compression(&mut self, compression: CompressionConfig) {
        self.compression = compression;
    }

    /// Sets the bytes the receive side may hold for messages it can not deliver yet: partial
    /// messages being reassembled, and ordered messages waiting behind a gap. A packet that
    /// would pass it is not accepted, so the session sends no ACK for it and the peer sends its
    /// reliable messages again once it finds the packet lost.
    ///
    /// It must be at least twice every receiving channel's `max_message_size`, the most one
    /// partial message can reserve, so that the largest message always completes.
    #[must_use]
    pub fn with_receive_budget(mut self, bytes: usize) -> Self {
        self.receive_budget = bytes;
        for receiver in &self.receivers {
            self.assert_budget_admits(receiver.max_message_size());
        }
        self
    }

    fn assert_budget_admits(&self, max_message_size: usize) {
        assert!(
            self.receive_budget >= max_message_size.saturating_mul(2),
            "a receive budget of {} bytes can not admit a channel's {max_message_size}-byte \
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
    pub(crate) fn heartbeat_due(&self, now: Instant) -> bool {
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
    /// A new session: its tick has no reference yet, and its largest payload, once it handed
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
            fragment_size,
            ..
        } = self;
        staged.retain(|message| {
            let max = senders[message.channel_id].max_message_size(*fragment_size);
            let fits = message.bytes.len() <= max;
            if !fits {
                refused.push(RefusedMessage {
                    kind: senders[message.channel_id].channel_kind(),
                    size: message.bytes.len(),
                    max,
                });
            }
            fits
        });
        Ok(refused)
    }

    /// The size the senders split a message's fragments to, which the largest payload of the
    /// link's session sets at each link.
    pub const fn fragment_size(&self) -> usize {
        self.fragment_size
    }

    /// Derives the senders' fragment size from the largest payload of the link's session.
    pub fn size_fragments(&mut self, max_payload: usize) -> Result<(), PacketError> {
        let fragment_size =
            fragment_size_for_max_payload(max_payload).ok_or(PacketError::PayloadTooSmall {
                actual: max_payload,
                min: MIN_FRAGMENT_PAYLOAD,
            })?;
        self.fragment_size = fragment_size;
        Ok(())
    }

    /// Returns the receiving channel registered for `channel_id`.
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
    pub fn add_channel_send<C: Channel>(&mut self, config: ChannelConfig, channel_id: NetId) {
        self.senders.insert(
            channel_id,
            ChannelSender::new(
                ChannelKind::of::<C>(),
                channel_id,
                core::any::type_name::<C>(),
                &config,
            ),
        );
    }

    /// Adds a manually configured receiving channel.
    ///
    /// An app's setup adds its channels from the channel registry, as `Transport` does.
    pub fn add_channel_receive<C: Channel>(&mut self, config: ChannelConfig, channel_id: NetId) {
        self.assert_budget_admits(config.max_message_size);
        self.receivers.insert(
            channel_id,
            ChannelReceiver::new(ChannelKind::of::<C>(), core::any::type_name::<C>(), &config),
        );
    }

    /// Stages a message on the sending channel `channel_id` for the next send.
    ///
    /// A message above the channel's `max_message_size` is refused with
    /// [`TransportError::MessageTooLarge`]; nothing is staged then.
    pub fn send(
        &mut self,
        channel_id: NetId,
        bytes: SendPayload,
        priority: f32,
    ) -> Result<(), TransportError> {
        self.check_message_size(channel_id, bytes.len())?;
        self.staged.push(StagedMessage {
            channel_id,
            bytes,
            priority,
        });
        Ok(())
    }

    /// Whether a message waits for the next send.
    pub const fn has_staged(&self) -> bool {
        !self.staged.is_empty()
    }

    fn check_message_size(&self, channel_id: NetId, size: usize) -> Result<(), TransportError> {
        let max = self
            .senders
            .get(channel_id)
            .ok_or(TransportError::ChannelNotFound(channel_id))?
            .max_message_size(self.fragment_size);
        if size > max {
            return Err(TransportError::MessageTooLarge { size, max });
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
            self.check_message_size(channel_id, bytes.len()).is_ok(),
            "a message is checked against its bound before it is buffered"
        );
        let Self {
            senders,
            compression_scratch,
            compression,
            fragment_size,
            ..
        } = self;
        let channel_sender = senders
            .get_mut(channel_id)
            .expect("a channel id names a sender of this transport");
        channel_sender.buffer_send(
            bytes,
            priority,
            *fragment_size,
            *compression,
            compression_scratch,
        )
    }

    /// The receive step at `now`, after a frame of `delta`: the channels' timers advance, each
    /// packet the session opened reaches its channels, the session accepts each one that did,
    /// and each sender learns the fate of each packet that carried its reliable messages.
    /// `admitted` hears the remote tick of each packet that reached its channels. A packet that
    /// breaks the protocol ends the reading, and its error comes back once the fates are taken.
    pub fn receive(
        &mut self,
        queues: &mut SessionQueues,
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
            // The session is the last receive stage that changes a payload in place. Freeze
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
    /// packets go to the session while its budget lasts; a flush that sends nothing past the
    /// heartbeat sends the tick alone. A session that handed over nothing yet takes nothing.
    pub fn flush(&mut self, queues: &mut SessionQueues, tick: NetTick, now: Instant) {
        // A link whose session has not handed over yet takes nothing: the staged messages
        // wait for the next frame.
        let max_payload = queues.max_payload();
        if max_payload == 0 {
            return;
        }
        self.packet_manager.begin_send(max_payload);
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
        self.priority_manager.clear();
        {
            let candidates = self.priority_manager.candidates_mut();
            self.senders.iter_mut().for_each(|channel| {
                channel.collect_send_candidates(candidates);
            });
        }
        self.priority_manager.prioritize();

        let encoded_tick = self.tick_encoder.encode(tick);
        let mut candidate_cursor = CandidateCursor::default();
        let mut total_bytes_sent: usize = 0;
        let mut first_token = None;
        let mut flush_outcome = SendFlushOutcome::Complete;
        loop {
            let source = PacketSource {
                tick: encoded_tick,
                candidates: self.priority_manager.candidates(),
                messages: &self.senders,
                compression: self.compression,
                max_payload,
            };
            let staged = self.packet_manager.build_next_packet(
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
            let num_messages = packet.num_messages();
            let packet_len = packet.len();
            let packet_compression = packet.compression;
            if packet_len > queues.budget() {
                // Staging has no channel side effects. Reliable messages are retained; each
                // unreliable channel applies its retry-unsent policy when this flush
                // finishes. Later candidates are not higher priority than this packet.
                flush_outcome = SendFlushOutcome::BudgetLimited;
                self.packet_manager.recycle_packet(packet);
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
                num_messages,
                compression_enabled = self.compression.is_enabled(),
                compression_algorithm = ?self.compression.algorithm,
                packet_compressed = packet_compression.is_some(),
                compression_original_len = packet_compression.map_or(0, |info| info.original_len),
                compression_compressed_len = packet_compression.map_or(0, |info| info.compressed_len),
                "sending transport packet"
            );

            // The session's queue is the transactional boundary: the channels' send state,
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
                channel_sender.sent_bytes += packet_message.num_bytes;
                if channel_sender.watches_acks() {
                    self.sent.push_message(SentMessage {
                        channel_id: commit.channel_id,
                        key: commit.key,
                    });
                }
            }
            self.packet_manager.recycle_packet_messages(packet_messages);
        }
        if first_token.is_none() && self.heartbeat_due(now) {
            let mut packet = self.packet_manager.build_tick_packet(encoded_tick);
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
                self.packet_manager
                    .recycle_packet_messages(core::mem::take(&mut packet.messages));
            } else {
                self.packet_manager.recycle_packet(packet);
            }
        }
        if first_token.is_some() {
            self.tick_encoder.on_sent(tick);
            self.last_sent = Some(now);
        }
        self.senders
            .iter_mut()
            .for_each(|channel| channel.finish_send(flush_outcome));
        self.priority_manager.clear();
        if total_bytes_sent > 0 {
            trace!(
                target: "campfire_net_debug::transport",
                kind = "send_flush",
                schedule = "PostUpdate",
                sample_point = "PostUpdate",
                local_tick = tick.0,
                sent_bytes = total_bytes_sent,
                "flushed transport packets to the session"
            );
        }
        self.sent_bytes.add(total_bytes_sent);
    }

    /// Resets the channels to their first state, as a link that disconnected needs.
    pub fn reset(&mut self) {
        self.receivers.iter_mut().for_each(ChannelReceiver::reset);
        self.senders.iter_mut().for_each(ChannelSender::reset);
        self.priority_manager = PriorityManager::default();
        self.packet_manager = PacketBuilder::default();
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
    use crate::channel_kind::ChannelKind;
    use crate::channel_receiver::ChannelReceiver;
    use crate::channel_sender::ChannelSender;
    use crate::channels::Channels;
    use crate::internals::channel_counts::ChannelCounts;
    use crate::internals::transport_backlog::TransportBacklog;
    use crate::internals::transport_counts::TransportCounts;
    use crate::message_id::MessageId;
    use crate::transport_error::TransportError;
    use campfire_net_core::NetId;
    use campfire_net_payload::SendPayload;

    impl Channels {
        /// Buffers bytes directly in the channel sender, as the next send does with each staged
        /// message.
        pub fn send_mut(
            &mut self,
            channel_id: NetId,
            bytes: SendPayload,
            priority: f32,
        ) -> Result<Option<MessageId>, TransportError> {
            self.check_message_size(channel_id, bytes.len())?;
            Ok(self.buffer_send(channel_id, bytes, priority))
        }

        /// The sending channel of `kind`.
        pub(crate) fn sender(&self, kind: ChannelKind) -> Option<&ChannelSender> {
            self.senders
                .iter()
                .find(|sender| sender.channel_kind() == kind)
        }

        /// Each reliable message not acknowledged yet, by its channel.
        pub fn pending_reliable(&self) -> Vec<(ChannelKind, MessageId)> {
            self.senders
                .iter()
                .flat_map(|sender| {
                    let kind = sender.channel_kind();
                    sender
                        .pending_reliable()
                        .into_iter()
                        .map(move |message| (kind, message))
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

        /// The bench counters of the channel `kind`, zero when the transport lacks a side of it.
        pub fn channel_counts(&self, kind: ChannelKind) -> ChannelCounts {
            ChannelCounts {
                sent_bytes: self
                    .sender(kind)
                    .map_or(0, |sender| sender.sent_bytes.get()),
                sent_messages: self
                    .sender(kind)
                    .map_or(0, |sender| sender.sent_messages.get()),
                received_bytes: self
                    .receivers
                    .iter()
                    .find(|receiver| receiver.channel_kind() == kind)
                    .map_or(0, |receiver| receiver.received_bytes.get()),
            }
        }
    }
}

#[cfg(test)]
mod tests;
