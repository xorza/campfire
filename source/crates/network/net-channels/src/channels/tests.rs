use super::*;

impl Channels {
    /// The sending channel `channel`.
    pub(crate) fn sender_mut(&mut self, channel: ChannelTypeId) -> Option<&mut ChannelSender> {
        self.senders
            .iter_mut()
            .find(|sender| sender.channel_type_id() == channel)
    }
}
#[cfg(feature = "compression_lz4")]
use crate::channel_message::ChannelMessage;
use crate::channel_mode::ChannelMode;
use crate::packet::MAX_FRAGMENTS;

/// The senders' fragment size follows the largest payload the packet session hands over: sizing for
/// 256 bytes, then linking at 200, gives each one's size; a link at 0 keeps the last.
#[test]
fn the_fragment_len_follows_the_packet_sessions_largest_payload() {
    let mut channels = Channels::default();
    channels.size_fragments(256).unwrap();
    assert_eq!(
        channels.fragment_len,
        fragment_len_for_max_payload(256).unwrap()
    );
    assert_eq!(channels.link(200).unwrap(), []);
    assert_eq!(
        channels.fragment_len,
        fragment_len_for_max_payload(200).unwrap()
    );
    assert_eq!(channels.link(0).unwrap(), []);
    assert_eq!(
        channels.fragment_len,
        fragment_len_for_max_payload(200).unwrap()
    );
}

/// A link whose payload makes the channel's message bound smaller drops each staged message
/// above the new bound and returns it, and keeps one at the bound: at a 200-byte payload, the
/// bound is the 16,383 fragments of a 200-byte payload's fragment size.
#[test]
fn a_link_refuses_the_staged_messages_its_bound_no_longer_holds() {
    let mut channels = Channels::default();
    let setup = ChannelSetup {
        max_message_len: 4 << 20,
        ..ChannelSetup::default()
    };
    channels.add_channel_send::<ResetChannel>(setup, NetId::new(0));
    channels.size_fragments(1200).unwrap();
    let max = MAX_FRAGMENTS * fragment_len_for_max_payload(200).unwrap();
    for len in [max + 1, max] {
        channels
            .send(NetId::new(0), Bytes::from(vec![0_u8; len]), 1.0)
            .unwrap();
    }

    assert_eq!(
        channels.link(200).unwrap(),
        [RefusedMessage {
            channel: ChannelTypeId::of::<ResetChannel>(),
            len: max + 1,
            max_len: max,
        }]
    );
    assert_eq!(
        channels
            .staged
            .iter()
            .map(|message| message.bytes.len())
            .collect::<Vec<_>>(),
        [max]
    );
}

/// A reset drops every pending message, and each channel keeps its own setup, so a reset needs
/// no registry.
#[test]
fn a_reset_drops_the_messages_and_keeps_each_channels_setup() {
    let mut channels = Channels::default();
    let mode = ChannelMode::OrderedReliable;
    let channel = ChannelSetup {
        mode,
        ..ChannelSetup::default()
    };
    channels.add_channel_send::<ResetChannel>(channel, NetId::new(0));
    channels.add_channel_receive::<ResetChannel>(channel, NetId::new(0));
    channels
        .send_mut(NetId::new(0), Bytes::from_static(b"m"), 1.0)
        .unwrap();
    assert_eq!(channels.pending_reliable().len(), 1);

    channels.reset();
    assert_eq!(channels.pending_reliable(), Vec::new());
    let sender = channels
        .sender(ChannelTypeId::of::<ResetChannel>())
        .unwrap();
    assert_eq!(sender.mode(), mode);
}

#[derive(Debug)]
struct ResetChannel;
impl Channel for ResetChannel {}

#[cfg(feature = "compression_lz4")]
mod lz4 {
    use super::*;

    use crate::message_ref::MessageRef;
    use crate::send_candidate::CandidateMessages;

    use crate::compression::tests::decompress;

    use crate::fragment_compression::FragmentCompression;

    use crate::packet::{DEFAULT_MAX_PAYLOAD, Packet};
    use crate::tick_encoder::LONG_TICK_LEN;
    use campfire_net_payload::OutgoingPayload;

    #[derive(Debug)]
    struct CompressionChannel;
    impl Channel for CompressionChannel {}

    #[test]
    fn a_compressed_fragmented_message_crosses_the_sender_the_builder_and_the_receiver()
    -> Result<(), PacketError> {
        let compression = CompressionSetup {
            min_payload_len: 0,
            max_decompressed_payload_len: FRAGMENT_LEN * 4,
            ..CompressionSetup::LZ4
        };
        let setup = ChannelSetup {
            mode: ChannelMode::SequencedUnreliable,
            ..ChannelSetup::default()
        };
        let channel_id = NetId::new(0);
        let channel_type_id = ChannelTypeId::of::<CompressionChannel>();

        let mut sender_transport = Channels::default();
        sender_transport.set_compression(compression);
        sender_transport.add_channel_send::<CompressionChannel>(setup, channel_id);

        let mut receiver_transport = Channels::default();
        receiver_transport.set_compression(compression);
        receiver_transport.add_channel_receive::<CompressionChannel>(setup, channel_id);

        let message = Bytes::from(vec![7u8; FRAGMENT_LEN * 3]);
        let message_id = sender_transport
            .send_mut(channel_id, message.clone(), 1.0)
            .unwrap()
            .unwrap();

        let mut candidates = vec![];
        sender_transport
            .sender_mut(channel_type_id)
            .unwrap()
            .collect_send_candidates(&mut candidates);
        // The compressed message is one candidate of fewer than 3 fragments, the first of which
        // names its compression.
        assert_eq!(candidates.len(), 1);
        let MessageRef::Fragment(first) = sender_transport
            .senders
            .message(channel_id, candidates[0].key)
        else {
            panic!("oversized message should be fragmented");
        };
        let first = first.data();
        assert_eq!(first.compression, Some(FragmentCompression::Lz4));
        assert!(first.fragments.0 < 3);

        let mut cursor = CandidateCursor::default();
        let mut packets = vec![];
        let sender_transport = &mut sender_transport;
        loop {
            let source = PacketSource {
                tick: TickEncoder::default().encode(NetTick(0)),
                candidates: &candidates,
                messages: &sender_transport.senders,
                compression,
                max_payload: DEFAULT_MAX_PAYLOAD,
            };
            let Some(packet) = sender_transport.packet_builder.build_next_packet(
                &source,
                &mut cursor,
                &mut sender_transport.compression_scratch,
            )?
            else {
                break;
            };
            packets.push(packet);
        }
        assert!(!packets.is_empty());

        for packet in packets {
            buffer_packet_into_receivers(&mut receiver_transport, packet, compression)?;
        }

        let receiver = receiver_transport.receivers.get_mut(channel_id).unwrap();
        assert_eq!(
            receiver.read_message(),
            Some(ChannelMessage {
                tick: NetTick(0),
                bytes: message,
                message_id: Some(message_id),
            })
        );
        Ok(())
    }

    /// Eight messages of 200 bytes pack into one packet whose length, which the packet session's
    /// budget takes, is its tick and its compressed body, below 600 bytes.
    #[test]
    fn a_packets_length_is_its_compressed_length() -> Result<(), PacketError> {
        let compression = CompressionSetup {
            min_payload_len: 0,
            ..CompressionSetup::LZ4
        };
        let setup = ChannelSetup {
            mode: ChannelMode::UnorderedUnreliable,
            ..ChannelSetup::default()
        };
        let channel_id = NetId::new(0);
        let mut channels = Channels::default();
        channels.set_compression(compression);
        channels.add_channel_send::<CompressionChannel>(setup, channel_id);

        for _ in 0..8 {
            channels
                .send_mut(channel_id, Bytes::from(vec![5u8; 200]), 1.0)
                .unwrap();
        }

        channels
            .senders
            .get_mut(channel_id)
            .unwrap()
            .collect_send_candidates(channels.candidate_order.candidates_mut());
        assert_eq!(channels.candidate_order.candidates().len(), 8);
        channels.candidate_order.prioritize();
        let mut cursor = CandidateCursor::default();
        let source = PacketSource {
            tick: TickEncoder::default().encode(NetTick(0)),
            candidates: channels.candidate_order.candidates(),
            messages: &channels.senders,
            compression,
            max_payload: DEFAULT_MAX_PAYLOAD,
        };
        let packet = channels
            .packet_builder
            .build_next_packet(&source, &mut cursor, &mut channels.compression_scratch)?
            .unwrap();

        assert_eq!(packet.messages.len(), 8);
        let compression = packet.compression.unwrap();
        assert_eq!(packet.len(), LONG_TICK_LEN + compression.compressed_len);
        assert!(packet.len() < 600);
        Ok(())
    }

    fn buffer_packet_into_receivers(
        channels: &mut Channels,
        packet: Packet,
        compression: CompressionSetup,
    ) -> Result<(), PacketError> {
        let mut payload = packet.payload.freeze();
        let _ = payload.split_to(OutgoingPayload::HEADROOM);
        let mut cursor = Reader::from(payload);
        let tick = TickDecoder::default().read(&mut cursor)?;
        let mut packet_kind = packet.kind;
        if packet_kind.is_compressed() {
            let compressed_payload = cursor.take_remaining();
            cursor = Reader::from(decompress(compressed_payload.as_ref(), compression)?);
            packet_kind = packet_kind.uncompressed_variant();
        }

        if packet_kind == PacketKind::DataFragment {
            let channel_id = NetId::from_bytes(&mut cursor)?;
            let fragment_body = FragmentBody::from_bytes(&mut cursor)?;
            channels
                .receivers
                .get_mut(channel_id)
                .ok_or(PacketError::ChannelNotFound)?
                .buffer_receive(
                    ReceiveMessage {
                        data: fragment_body.into(),
                        remote_sent_tick: tick,
                        compression,
                    },
                    &mut ReceiveBudget::new(usize::MAX),
                )?;
        }

        while cursor.has_remaining() {
            let channel_id = NetId::from_bytes(&mut cursor)?;
            let messages = cursor.read_u8().map_err(SerializationError::from)?;
            let receiver = channels
                .receivers
                .get_mut(channel_id)
                .ok_or(PacketError::ChannelNotFound)?;
            let has_id = receiver.single_has_id();
            for _ in 0..messages {
                let single_body = SingleBody::read(&mut cursor, has_id)?;
                receiver.buffer_receive(
                    ReceiveMessage {
                        data: single_body.into(),
                        remote_sent_tick: tick,
                        compression,
                    },
                    &mut ReceiveBudget::new(usize::MAX),
                )?;
            }
        }
        Ok(())
    }
}

/// A 10-byte limit admits 10 bytes and refuses 11, on both send paths, staging nothing.
#[test]
fn a_send_above_the_channel_limit_is_refused() {
    let setup = ChannelSetup {
        max_message_len: 10,
        ..ChannelSetup::default()
    };
    let mut channels = Channels::default();
    let id = NetId::new(0);
    channels.add_channel_send::<ResetChannel>(setup, id);

    assert!(channels.send(id, Bytes::from(vec![0; 10]), 1.0).is_ok());
    assert!(matches!(
        channels.send(id, Bytes::from(vec![0; 11]), 1.0),
        Err(TransportError::MessageTooLarge {
            len: 11,
            max_len: 10
        })
    ));
    assert_eq!(channels.staged.len(), 1);
    assert!(channels.send_mut(id, Bytes::from(vec![0; 10]), 1.0).is_ok());
    assert!(matches!(
        channels.send_mut(id, Bytes::from(vec![0; 11]), 1.0),
        Err(TransportError::MessageTooLarge {
            len: 11,
            max_len: 10
        })
    ));
    // A channel the transport does not send on is refused too.
    assert!(matches!(
        channels.send(NetId::new(1), Bytes::from(vec![0; 1]), 1.0),
        Err(TransportError::ChannelNotFound(missing)) if missing == NetId::new(1)
    ));
}

/// A 100-byte channel needs a budget of at least 200 bytes, the most its partial can reserve.
#[test]
#[should_panic(expected = "it needs twice that")]
fn a_budget_below_twice_the_largest_message_is_refused() {
    let setup = ChannelSetup {
        max_message_len: 100,
        ..ChannelSetup::default()
    };
    let mut channels = Channels::default().with_receive_budget(199);
    channels.add_channel_receive::<ResetChannel>(setup, NetId::new(0));
}

/// The steps' tests: what a flush gives a packet session's queues, and what a receive takes from
/// them.
mod steps {
    use super::*;
    use crate::packet::DEFAULT_MAX_PAYLOAD;
    use crate::tick_encoder::LONG_TICK_MARK;
    use bytes::BytesMut;
    use campfire_net_payload::ReceivePayload;

    #[derive(Debug)]
    struct RetryChannel;
    impl Channel for RetryChannel {}
    #[derive(Debug)]
    struct DiscardChannel;
    impl Channel for DiscardChannel {}
    #[derive(Debug)]
    struct SmallPayloadChannel;
    impl Channel for SmallPayloadChannel {}
    #[derive(Debug)]
    struct FateChannel;
    impl Channel for FateChannel {}
    #[derive(Debug)]
    struct BudgetChannel;
    impl Channel for BudgetChannel {}
    #[derive(Debug)]
    struct OrderedChannel;
    impl Channel for OrderedChannel {}

    fn at(millis: u64) -> Instant {
        Instant::from_elapsed(Duration::from_millis(millis))
    }

    /// A packet session's queues that carry packets of `DEFAULT_MAX_PAYLOAD` within a budget of
    /// `budget`.
    fn queues(budget: usize) -> PacketSessionQueues {
        let mut queues = PacketSessionQueues::default();
        queues.set_room(budget, 0);
        queues.set_max_payload(DEFAULT_MAX_PAYLOAD);
        queues
    }

    /// A receive at 0 that hears nothing of what it admits.
    fn receive(
        channels: &mut Channels,
        queues: &mut PacketSessionQueues,
    ) -> Result<(), TransportError> {
        channels.receive(queues, Instant::ZERO, Duration::ZERO, |_| {})
    }

    fn pending_candidates<C: Channel>(channels: &mut Channels) -> usize {
        let mut candidates = Vec::new();
        channels
            .sender_mut(ChannelTypeId::of::<C>())
            .unwrap()
            .collect_send_candidates(&mut candidates);
        candidates.len()
    }

    /// A payload the frame got from its packet session, as `kind` after its headroom.
    fn opened(number: u64, kind: PacketKind, data: &[u8]) -> OpenedPayload {
        OpenedPayload {
            number: PacketNumber(number),
            kind: kind.into(),
            payload: ReceivePayload::Mutable(BytesMut::from(data)),
        }
    }

    /// A data packet whose `body` follows the long tick 1.
    fn data_payload(body: &[u8]) -> Vec<u8> {
        let mut payload = vec![LONG_TICK_MARK, 0, 0, 0, 1];
        payload.extend_from_slice(body);
        payload
    }

    /// A flush that the packet session's budget stops after one packet keeps the second message of
    /// a channel that retries unsent messages, and drops it on one that does not. One packet of
    /// 1,000 bytes of message takes 1,009: a long tick of 5, the channel id, the count, a length of
    /// 2 and the message. A budget of 1,100 leaves 91 after it.
    #[test]
    fn a_limited_flush_applies_each_channels_retry_policy() {
        let retry = ChannelSetup {
            mode: ChannelMode::UnorderedUnreliable,
            retry_unsent_messages: true,
            ..Default::default()
        };
        let discard = ChannelSetup {
            retry_unsent_messages: false,
            ..retry
        };
        let flushed = |add: fn(&mut Channels, ChannelSetup), setup| {
            let mut channels = Channels::default();
            add(&mut channels, setup);
            for value in [1, 2] {
                channels
                    .send_mut(NetId::new(0), Bytes::from(vec![value; 1000]), 1.0)
                    .unwrap();
            }
            let mut queues = queues(1_100);
            channels.flush(&mut queues, NetTick(0), Instant::ZERO);
            let sends: Vec<_> = queues.take_sends().collect();
            assert_eq!(sends.len(), 1);
            assert_eq!(sends[0].data().len(), 1_009);
            channels
        };
        let mut kept = flushed(
            |channels, setup| channels.add_channel_send::<RetryChannel>(setup, NetId::new(0)),
            retry,
        );
        assert_eq!(pending_candidates::<RetryChannel>(&mut kept), 1);
        let mut dropped = flushed(
            |channels, setup| channels.add_channel_send::<DiscardChannel>(setup, NetId::new(0)),
            discard,
        );
        assert_eq!(pending_candidates::<DiscardChannel>(&mut dropped), 0);
    }

    /// A reliable message whose packet the packet session finds lost goes again in the next flush,
    /// as a resend under a new token; the delivery of that packet ends it. A delivery tells the
    /// tick's encoder too, whose next packet carries the short tick.
    #[test]
    fn a_lost_packet_sends_its_reliable_messages_again() {
        let setup = ChannelSetup {
            mode: ChannelMode::OrderedReliable,
            ..Default::default()
        };
        let mut channels = Channels::default();
        channels.add_channel_send::<FateChannel>(setup, NetId::new(0));
        channels
            .send(NetId::new(0), Bytes::from_static(b"m"), 1.0)
            .unwrap();
        let mut queues = queues(usize::MAX);
        let flush = |channels: &mut Channels, queues: &mut PacketSessionQueues| {
            channels.flush(queues, NetTick(0), Instant::ZERO);
            queues.take_sends().collect::<Vec<_>>()
        };

        let first = flush(&mut channels, &mut queues);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].token, PayloadToken(0));
        assert_eq!(first[0].data()[0], LONG_TICK_MARK);
        assert!(
            flush(&mut channels, &mut queues).is_empty(),
            "a message in flight waits"
        );

        queues.push_fate(PayloadFate::Lost(PayloadToken(0)));
        receive(&mut channels, &mut queues).unwrap();
        let second = flush(&mut channels, &mut queues);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].token, PayloadToken(1));
        assert_eq!(channels.counts().resent_messages, 1);

        queues.push_fate(PayloadFate::Delivered(PayloadToken(1)));
        receive(&mut channels, &mut queues).unwrap();
        assert!(channels.pending_reliable().is_empty());
        assert_eq!(channels.backlog().tracked_packet_acks, 0);
        assert_eq!(channels.tick_encoder.encode(NetTick(0)).as_bytes(), [0, 0]);
    }

    /// An idle transport sends its tick: a flush with no message sends a packet of the tick
    /// alone, a long one of 5 bytes, when 100 ms passed since its last packet, and none before.
    /// A message sent in between counts as a packet: the tick, the channel id, the count, the
    /// length and the byte, 5 + 1 + 1 + 1 + 1.
    #[test]
    fn an_idle_transport_sends_its_tick() {
        let mut channels = Channels::default();
        channels.add_channel_send::<FateChannel>(ChannelSetup::default(), NetId::new(0));
        let mut queues = queues(usize::MAX);
        let mut flush_at = |channels: &mut Channels, millis| {
            channels.flush(&mut queues, NetTick(0), at(millis));
            queues
                .take_sends()
                .map(|send| send.data().len())
                .collect::<Vec<_>>()
        };
        assert_eq!(flush_at(&mut channels, 0), [5]);
        assert_eq!(flush_at(&mut channels, 99), Vec::<usize>::new());
        assert_eq!(flush_at(&mut channels, 100), [5]);
        channels
            .send(NetId::new(0), Bytes::from_static(b"m"), 1.0)
            .unwrap();
        assert_eq!(flush_at(&mut channels, 150), [9]);
        assert_eq!(flush_at(&mut channels, 249), Vec::<usize>::new());
        assert_eq!(flush_at(&mut channels, 250), [5]);
    }

    /// The two links' packet sessions carry payloads of different sizes, so the sender's fragment
    /// size differs from the one the receiver's own link gives.
    #[test]
    fn fragments_follow_the_sender_payload_and_reassemble_on_any_link() {
        for (sender_payload, receiver_payload) in
            [(256, DEFAULT_MAX_PAYLOAD), (DEFAULT_MAX_PAYLOAD, 256)]
        {
            let setup = ChannelSetup::default();
            let channel_id = NetId::new(0);
            let mut sender = Channels::default();
            sender.add_channel_send::<SmallPayloadChannel>(setup, channel_id);
            sender.link(sender_payload).unwrap();
            let mut receiver = Channels::default();
            receiver.add_channel_receive::<SmallPayloadChannel>(setup, channel_id);
            receiver.link(receiver_payload).unwrap();
            let mut sender_queues = queues(usize::MAX);
            sender_queues.set_max_payload(sender_payload);
            let mut receiver_queues = queues(usize::MAX);
            receiver_queues.set_max_payload(receiver_payload);

            let expected = Bytes::from(vec![5; 3000]);
            sender.send(channel_id, expected.clone(), 1.0).unwrap();
            sender.flush(&mut sender_queues, NetTick(0), Instant::ZERO);
            let sends: Vec<_> = sender_queues.take_sends().collect();
            assert!(sends.len() > 1);
            assert!(sends.iter().all(|send| send.data().len() <= sender_payload));

            for (number, send) in (0..).zip(sends) {
                receiver_queues.push_opened(OpenedPayload {
                    number: PacketNumber(number),
                    kind: send.kind,
                    payload: ReceivePayload::Mutable(BytesMut::from(send.data())),
                });
            }
            receive(&mut receiver, &mut receiver_queues).unwrap();
            let received = receiver
                .receivers
                .get_mut(channel_id)
                .unwrap()
                .read_message()
                .unwrap()
                .bytes;
            assert_eq!(received, expected, "sender payload {sender_payload}");
        }
    }

    /// Bytes that do not decode, a channel the transport lacks, a short tick with no whole one
    /// before it and the fourth payload kind each break the protocol: the receive returns the
    /// error, admits nothing and accepts nothing, so the packet session never acknowledges the
    /// packet, and the packet queued behind it is not read. 0xff starts an 8-byte varint channel id
    /// with no bytes after it; channel 5 carries one message with no id, a length of 1 and the byte
    /// 7.
    #[test]
    fn a_protocol_violation_admits_and_accepts_nothing() {
        let undecodable = opened(0, PacketKind::Data, &data_payload(&[0xff]));
        let no_channel = opened(0, PacketKind::Data, &data_payload(&[5, 1, 1, 7]));
        let short_tick = opened(0, PacketKind::Data, &[0, 1]);
        let fourth_kind = OpenedPayload {
            kind: PayloadKind::new(3),
            ..opened(0, PacketKind::Data, &data_payload(&[]))
        };
        for violation in [undecodable, no_channel, short_tick, fourth_kind] {
            let mut channels = Channels::default();
            let mut queues = queues(usize::MAX);
            queues.push_opened(violation.clone());
            queues.push_opened(opened(1, PacketKind::Data, &data_payload(&[])));
            let mut admitted = 0;
            let received = channels.receive(&mut queues, Instant::ZERO, Duration::ZERO, |_| {
                admitted += 1;
            });
            assert!(received.is_err(), "{violation:?}");
            assert_eq!(admitted, 0, "{violation:?}");
            assert_eq!(queues.take_accepted().count(), 0, "{violation:?}");
        }
    }

    /// One batch of messages on channel 0, each with its id as an ordered channel sends it.
    fn batch(messages: &[(u16, &[u8])]) -> Vec<u8> {
        let mut payload = vec![0, u8::try_from(messages.len()).unwrap()];
        for &(id, bytes) in messages {
            SingleBody::new(Some(MessageId(id)), Bytes::copy_from_slice(bytes)).write(&mut payload);
        }
        data_payload(&payload)
    }

    /// An ordered channel of 100-byte messages under a 200-byte budget: messages 1 and 2 fill it
    /// behind the gap of message 0, so the packet with message 3 is not accepted, and gets no
    /// ACK. Once message 0 arrives and the three are read, the same packet is admitted. Of the
    /// four payloads the frame read, it accepts the three it admitted.
    #[test]
    fn a_packet_over_the_receive_budget_is_not_accepted() {
        let setup = ChannelSetup {
            mode: ChannelMode::OrderedReliable,
            max_message_len: 100,
            ..Default::default()
        };
        let mut channels = Channels::default().with_receive_budget(200);
        channels.add_channel_receive::<BudgetChannel>(setup, NetId::new(0));
        let mut queues = queues(usize::MAX);
        let mut read = |channels: &mut Channels, payloads: &[OpenedPayload]| {
            for payload in payloads {
                queues.push_opened(payload.clone());
            }
            receive(channels, &mut queues).unwrap();
            queues.take_accepted().collect::<Vec<_>>()
        };
        let full = opened(
            0,
            PacketKind::Data,
            &batch(&[(1, &[1; 100]), (2, &[2; 100])]),
        );
        let over = opened(1, PacketKind::Data, &batch(&[(3, &[3])]));
        let head = opened(2, PacketKind::Data, &batch(&[(0, &[0])]));
        assert_eq!(
            read(&mut channels, &[full, over.clone(), head]),
            [PacketNumber(0), PacketNumber(2)]
        );

        let receiver = channels.receivers.get_mut(NetId::new(0)).unwrap();
        let delivered = core::iter::from_fn(|| receiver.read_message())
            .map(|message| message.message_id.unwrap())
            .collect::<Vec<_>>();
        assert_eq!(delivered, [MessageId(0), MessageId(1), MessageId(2)]);

        assert_eq!(read(&mut channels, &[over]), [PacketNumber(1)]);
        let receiver = channels.receivers.get_mut(NetId::new(0)).unwrap();
        assert_eq!(
            receiver.read_message().unwrap().message_id,
            Some(MessageId(3))
        );
    }

    /// A seeded mutation test of the packet parser: 2,048 mutations of each packet of a flush,
    /// with single messages of an unordered and an ordered channel and a message of three
    /// fragments, reach a receiver of the same channels, and none panics; a mutation reads, or
    /// breaks the protocol, or is refused at the receive budget.
    #[test]
    fn a_mutated_packet_is_refused_or_read() {
        use campfire_net_bytes::internals::Mutations;

        let unordered = ChannelSetup::default();
        let ordered = ChannelSetup {
            mode: ChannelMode::OrderedReliable,
            ..ChannelSetup::default()
        };
        let [unordered_id, ordered_id] = [NetId::new(0), NetId::new(1)];
        let mut sender = Channels::default();
        sender.add_channel_send::<FateChannel>(unordered, unordered_id);
        sender.add_channel_send::<OrderedChannel>(ordered, ordered_id);
        let mut receiver = Channels::default();
        receiver.add_channel_receive::<FateChannel>(unordered, unordered_id);
        receiver.add_channel_receive::<OrderedChannel>(ordered, ordered_id);
        for (channel_id, bytes) in [
            (unordered_id, vec![1; 40]),
            (ordered_id, vec![2; 40]),
            (ordered_id, vec![3; 2_500]),
        ] {
            sender.send(channel_id, Bytes::from(bytes), 1.0).unwrap();
        }
        let mut queues = queues(usize::MAX);
        sender.flush(&mut queues, NetTick(0), Instant::ZERO);
        let sends: Vec<_> = queues
            .take_sends()
            .map(|send| (send.kind, send.data().to_vec()))
            .collect();
        assert_eq!(
            sends.len(),
            3,
            "three fragments, the singles in the room of the last"
        );

        let mut mutations = Mutations::new(0x7a_c0de);
        let mut mutated = Vec::new();
        let [mut read, mut broken] = [0; 2];
        for (kind, original) in &sends {
            for _ in 0..2_048 {
                mutations.mutate(original, &mut mutated);
                match receive_packet(&mut receiver, *kind, Bytes::from(mutated.clone())) {
                    Ok(_) => read += 1,
                    Err(_) => broken += 1,
                }
            }
        }
        assert!(read > 0 && broken > 0, "{read} read, {broken} broken");
    }
}
