use crate::bench_counter::BenchCounter;
use crate::candidate_order::CandidateOrder;
use crate::channel_reader::ChannelReader;
use crate::channel_receiver::BufferOutcome;
use crate::channel_receiver::ChannelReceiver;
use crate::channel_sender::ChannelSender;
use crate::channel_sender::SendFlushOutcome;
use crate::channel_slots::ChannelSlots;
use crate::channel_type_id::ChannelTypeId;
use crate::error::{ReceiveError, SendError};
use crate::fragment_body::FragmentBody;
use crate::message_id::MessageId;
use crate::packet::{
    FRAGMENT_LEN, MAX_FRAGMENTS, MIN_FRAGMENT_PAYLOAD, fragment_len_for_max_payload,
};
use crate::packet_builder::PacketBuilder;
use crate::packet_builder::{CandidateCursor, PacketSource};
use crate::packet_kind::PacketKind;
use crate::receive_budget::{DEFAULT_RECEIVE_BUDGET, ReceiveBudget};
use crate::send_budget::SEND_BUDGET;
use crate::single_body::SingleBody;
use crate::tick_decoder::TickDecoder;
use crate::tick_encoder::TickEncoder;
use bytes::Bytes;
use campfire_net_bytes::{Reader, ToBytes};
use campfire_net_core::Instant;
use campfire_net_core::{NetId, TickInstant};
use campfire_net_payload::{
    OpenedPayload, PacketSessionQueues, PayloadFate, PayloadKind, PayloadToken,
};
use core::time::Duration;
use tracing::trace;

use crate::channel::Channel;
use campfire_net_payload::SendPayload;

use crate::channel_setup::ChannelSetup;
use crate::sent_packets::{SentMessage, UnsettledPackets};

/// A message that [`Channels::send`] staged for the next send (11-net-stack/transport.md, A queued
/// message keeps its channel's type until the flush).
#[derive(Debug)]
pub(crate) struct StagedMessage {
    pub(crate) channel_id: NetId,
    pub(crate) bytes: Bytes,
    pub(crate) priority: f32,
}

/// What became of one received packet whose bytes keep the protocol; a protocol violation is the
/// `Err` of `receive_packet`, which closes the link (11-net-stack/transport.md, A received packet
/// ends one of three ways).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PacketOutcome {
    /// The payload reached its channels, and the frame accepts the packet, which the packet session
    /// then acknowledges.
    Admitted { remote: TickInstant },
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
) -> Result<PacketOutcome, ReceiveError> {
    let packet_len = packet.len();
    channels.received_bytes.add(packet_len);
    let packet_kind = PacketKind::try_from(kind)?;
    let mut cursor = Reader::from(packet);
    let remote = channels.tick_decoder.read(&mut cursor)?;
    let tick = remote.tick();
    trace!(
        target: "campfire_net_debug::transport",
        kind = "packet_recv",
        remote_tick = tick.0,
        bytes = packet_len,
        ?packet_kind,
        "received transport packet"
    );

    let mut budget = channels.receive_room();

    if packet_kind == PacketKind::DataFragment {
        let channel_id = NetId::from_bytes(&mut cursor)?;
        let fragment_body = FragmentBody::from_bytes(&mut cursor)?;
        trace!(
            target: "campfire_net_debug::transport",
            kind = "channel_recv_fragment",
            remote_tick = tick.0,
            channel_id = channel_id.get(),
            bytes = fragment_body.bytes.len(),
            "received channel fragment"
        );
        let receiver = channels
            .receivers
            .get_mut(channel_id)
            .ok_or(ReceiveError::UnknownChannel(channel_id))?;
        receiver.received_bytes.add(fragment_body.bytes.len());
        if receiver.receive_fragment(fragment_body, tick, &mut budget)? == BufferOutcome::OverBudget
        {
            return Ok(PacketOutcome::OverBudget);
        }
    }
    while cursor.has_remaining() {
        let channel_id = NetId::from_bytes(&mut cursor)?;
        let messages = cursor.read_u8()?;
        if messages == 0 {
            return Err(ReceiveError::EmptyBatch);
        }
        trace!(
            target: "campfire_net_debug::transport",
            kind = "channel_recv_batch",
            remote_tick = tick.0,
            channel_id = channel_id.get(),
            messages,
            "received channel message batch"
        );
        let receiver = channels
            .receivers
            .get_mut(channel_id)
            .ok_or(ReceiveError::UnknownChannel(channel_id))?;
        let has_id = receiver.single_has_id();
        for _ in 0..messages {
            let single_body = SingleBody::read(&mut cursor, has_id)?;
            trace!(
                target: "campfire_net_debug::transport",
                kind = "channel_recv_message",
                remote_tick = tick.0,
                channel_id = channel_id.get(),
                bytes = single_body.bytes.len(),
                "received channel message bytes"
            );
            receiver.received_bytes.add(single_body.bytes.len());
            if receiver.receive_single(single_body, tick, &mut budget) == BufferOutcome::OverBudget
            {
                return Ok(PacketOutcome::OverBudget);
            }
        }
    }
    Ok(PacketOutcome::Admitted { remote })
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
    /// The longest the transport stays silent: past it, a flush with no message sends a packet
    /// of its tick alone.
    heartbeat: Duration,
    /// When the transport last gave its packet session a packet.
    pub(crate) last_sent: Option<Instant>,
    /// Fragment payload size of the senders, derived from the largest payload of the link's
    /// packet session.
    fragment_len: usize,
    /// The bytes the receive side may hold for messages it can not deliver yet: partial messages
    /// being reassembled, and ordered messages waiting behind a gap. A packet that would pass it
    /// is not accepted, so the packet session sends no ACK for it and the peer sends its reliable
    /// messages again once it finds the packet lost.
    receive_budget: usize,

    /// The packets in flight: the tick of each, and its reliable messages, so that a sender learns
    /// each one's fate. The packet session reports each packet's fate once, so an entry lives until
    /// its packet is delivered or lost.
    pub(crate) sent: UnsettledPackets,
    /// Messages staged for the next send, in staging order (11-net-stack/transport.md, A link's
    /// `Channels` queue its sends in a `Vec`, in the `Transport` component).
    pub(crate) staged: Vec<StagedMessage>,
    /// The bytes of the messages of `staged`.
    staged_len: usize,
    /// Bytes of the packets given to the packet session.
    pub(crate) sent_bytes: BenchCounter,
    /// Packets given to the packet session.
    pub(crate) sent_packets: BenchCounter,
    /// Messages and fragments that a packet given to the packet session carried again.
    pub(crate) resent_messages: BenchCounter,
    /// Bytes of the packets the packet session opened.
    pub(crate) received_bytes: BenchCounter,
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
            heartbeat: Self::DEFAULT_HEARTBEAT,
            last_sent: None,
            fragment_len: FRAGMENT_LEN,
            receive_budget: DEFAULT_RECEIVE_BUDGET,
            sent: UnsettledPackets::default(),
            staged: Vec::new(),
            staged_len: 0,
            sent_bytes: BenchCounter::default(),
            sent_packets: BenchCounter::default(),
            resent_messages: BenchCounter::default(),
            received_bytes: BenchCounter::default(),
        }
    }
}

impl Channels {
    /// The default [`Self::set_heartbeat`]: 100 ms, the interval the ping messages kept before
    /// the packet session gave the RTT.
    pub const DEFAULT_HEARTBEAT: Duration = Duration::from_millis(100);

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
    /// give neither (11-net-stack/transport.md, An idle transport sends its tick).
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
    /// one over, sizes the fragments.
    pub fn link(&mut self, max_payload: usize) {
        self.tick_encoder.reset();
        self.tick_decoder.reset();
        if max_payload > 0 {
            self.size_fragments(max_payload);
        }
    }

    /// The size the senders split a message's fragments to, which the largest payload of the
    /// link's packet session sets at each link.
    pub const fn fragment_len(&self) -> usize {
        self.fragment_len
    }

    /// Derives the senders' fragment size from the largest payload of the link's packet session,
    /// which holds every sending channel's largest message in [`MAX_FRAGMENTS`] fragments.
    pub fn size_fragments(&mut self, max_payload: usize) {
        self.fragment_len = fragment_len_for_max_payload(max_payload).unwrap_or_else(|| {
            panic!(
                "a payload of {max_payload} bytes holds no fragment: it needs \
                 {MIN_FRAGMENT_PAYLOAD}"
            )
        });
        for sender in &self.senders {
            self.assert_fragments_hold(sender.max_message_len());
        }
    }

    fn assert_fragments_hold(&self, max_message_len: usize) {
        assert!(
            max_message_len <= MAX_FRAGMENTS * self.fragment_len,
            "a channel's {max_message_len}-byte message passes {MAX_FRAGMENTS} fragments of {}",
            self.fragment_len
        );
    }

    /// The receiving channel registered for `channel_id`.
    pub fn channel_receiver_mut(&mut self, channel_id: NetId) -> Option<&mut ChannelReceiver> {
        self.receivers.get_mut(channel_id)
    }

    /// Mutably iterates over the registered receiving channels.
    pub fn channel_receivers_mut(&mut self) -> impl Iterator<Item = &mut ChannelReceiver> {
        self.receivers.iter_mut()
    }

    /// Adds the sending channel of `C` at `channel_id`, unless the transport sends on one there.
    pub fn add_channel_send<C: Channel>(&mut self, setup: ChannelSetup, channel_id: NetId) {
        self.open_send(
            ChannelTypeId::of::<C>(),
            core::any::type_name::<C>(),
            setup,
            channel_id,
        );
    }

    /// Adds the receiving channel of `C` at `channel_id`, unless the transport receives on one
    /// there.
    pub fn add_channel_receive<C: Channel>(&mut self, setup: ChannelSetup, channel_id: NetId) {
        self.open_receive(
            ChannelTypeId::of::<C>(),
            core::any::type_name::<C>(),
            setup,
            channel_id,
            ChannelReader::Messages,
        );
    }

    /// Adds the sending channel `channel`, of the type named `name`, at `channel_id`, unless the
    /// transport sends on one there, which keeps its state.
    pub fn open_send(
        &mut self,
        channel: ChannelTypeId,
        name: &'static str,
        setup: ChannelSetup,
        channel_id: NetId,
    ) {
        if self.senders.get(channel_id).is_some() {
            return;
        }
        self.assert_fragments_hold(setup.max_message_len);
        self.senders.insert(
            channel_id,
            ChannelSender::new(channel, channel_id, name, &setup),
        );
    }

    /// Adds the receiving channel `channel`, as [`Self::open_send`] adds a sending one.
    pub fn open_receive(
        &mut self,
        channel: ChannelTypeId,
        name: &'static str,
        setup: ChannelSetup,
        channel_id: NetId,
        reader: ChannelReader,
    ) {
        if self.receivers.get(channel_id).is_some() {
            return;
        }
        let receiver = ChannelReceiver::new(channel, name, &setup, reader);
        self.receivers.insert(channel_id, receiver);
    }

    /// Stages a message on the sending channel `channel_id` for the next send.
    ///
    /// A message above the channel's `max_message_len` is refused with
    /// [`SendError::MessageTooLarge`], and one that would pass the link's send budget with
    /// [`SendError::Backlogged`]; nothing is staged then.
    pub fn send(
        &mut self,
        channel_id: NetId,
        bytes: SendPayload,
        priority: f32,
    ) -> Result<(), SendError> {
        self.check_send(channel_id, bytes.len())?;
        self.staged_len += bytes.len();
        self.staged.push(StagedMessage {
            channel_id,
            bytes,
            priority,
        });
        Ok(())
    }

    /// Whether a message of `len` bytes may go on the sending channel `channel_id`: within the
    /// channel's bound and the link's send budget (11-net-stack/transport.md, A link's queued
    /// bytes have a budget).
    fn check_send(&self, channel_id: NetId, len: usize) -> Result<(), SendError> {
        let max_len = self
            .senders
            .get(channel_id)
            .ok_or(SendError::ChannelNotFound(channel_id))?
            .max_message_len();
        if len > max_len {
            return Err(SendError::MessageTooLarge { len, max_len });
        }
        let queued = self.queued();
        if queued + len > SEND_BUDGET {
            return Err(SendError::Backlogged { len, queued });
        }
        Ok(())
    }

    /// The bytes the link queues: its staged messages, and what its senders hold until they send
    /// them, or on a reliable channel until they are acknowledged.
    fn queued(&self) -> usize {
        self.staged_len
            + self
                .senders
                .iter()
                .map(ChannelSender::queued)
                .sum::<usize>()
    }

    /// Buffers bytes in the sender of `channel_id`, a channel this transport sends on, of a size
    /// its bound was checked against at staging.
    fn buffer_send(
        &mut self,
        channel_id: NetId,
        bytes: SendPayload,
        priority: f32,
    ) -> Option<MessageId> {
        debug_assert!(
            bytes.len() <= self.senders[channel_id].max_message_len(),
            "a message is checked against its bound before it is buffered"
        );
        let fragment_len = self.fragment_len;
        self.senders
            .get_mut(channel_id)
            .expect("a channel id names a sender of this transport")
            .buffer_send(bytes, priority, fragment_len)
    }

    /// The receive step at `now`, after a frame of `delta`: the channels' timers advance, each
    /// packet the packet session opened reaches its channels, the packet session accepts each one
    /// that did, and each sender learns the fate of each packet that carried its reliable messages.
    /// It returns the latest remote instant of a packet that reached its channels, if one did. A
    /// packet that breaks the protocol ends the reading, and its error comes back once the fates
    /// are taken.
    pub fn receive(
        &mut self,
        queues: &mut PacketSessionQueues,
        now: Instant,
        delta: Duration,
    ) -> Result<Option<TickInstant>, ReceiveError> {
        self.senders
            .iter_mut()
            .for_each(|channel_sender| channel_sender.update(delta));
        self.receivers
            .iter_mut()
            .for_each(|channel_receiver| channel_receiver.update(now));
        let mut violation = None;
        let mut largest = None;
        while let Some(OpenedPayload {
            number,
            kind,
            payload,
        }) = queues.pop_opened()
        {
            // The packet session is the last receive stage that changes a payload in place. Freeze
            // here so transport parsing can retain cheap immutable subslices.
            match receive_packet(self, kind, payload.freeze()) {
                Ok(PacketOutcome::Admitted { remote }) => {
                    queues.accept(number);
                    largest = largest.max(Some(remote));
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
        if violation.is_some() {
            // The link ends: its other packets go unread.
            drop(queues.opened());
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
        violation.map_or(Ok(largest), Err)
    }

    /// The send step at `now`, at `at` on the sender's timeline: each staged message enters its
    /// channel, and packets go to the packet session while its budget lasts; a flush that sends
    /// nothing past the heartbeat sends the instant alone. A packet session that handed over
    /// nothing yet takes nothing.
    pub fn flush(&mut self, queues: &mut PacketSessionQueues, at: TickInstant, now: Instant) {
        let tick = at.tick();
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
        self.staged_len = 0;

        // Collect candidates while each channel keeps its pending queues.
        self.candidate_order.clear();
        {
            let candidates = self.candidate_order.candidates_mut();
            self.senders.iter_mut().for_each(|channel| {
                channel.collect_send_candidates(candidates);
            });
        }
        self.candidate_order.prioritize();

        let encoded_tick = self.tick_encoder.encode(at);
        let mut candidate_cursor = CandidateCursor::default();
        let mut total_bytes_sent: usize = 0;
        let mut first_token = None;
        let mut flush_outcome = SendFlushOutcome::Complete;
        loop {
            let source = PacketSource {
                tick: encoded_tick,
                candidates: self.candidate_order.candidates(),
                messages: &self.senders,
                max_payload,
            };
            let Some(mut packet) = self
                .packet_builder
                .build_next_packet(&source, &mut candidate_cursor)
            else {
                break;
            };
            let messages = packet.messages.len();
            let packet_len = packet.len();
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
                ?token,
                local_tick = tick.0,
                bytes = packet_len,
                messages,
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
        self.last_sent = None;
        self.sent.clear();
        self.staged.clear();
        self.staged_len = 0;
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::channel_receiver::ChannelReceiver;
    use crate::channel_sender::ChannelSender;
    use crate::channel_type_id::ChannelTypeId;
    use crate::channels::Channels;
    use crate::error::SendError;
    use crate::message_id::MessageId;
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
    }

    impl Add for TransportCounts {
        type Output = Self;

        fn add(self, rhs: Self) -> Self {
            Self {
                sent_bytes: self.sent_bytes + rhs.sent_bytes,
                sent_packets: self.sent_packets + rhs.sent_packets,
                resent_messages: self.resent_messages + rhs.resent_messages,
                received_bytes: self.received_bytes + rhs.received_bytes,
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
        ) -> Result<Option<MessageId>, SendError> {
            self.check_send(channel_id, bytes.len())?;
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
