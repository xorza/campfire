use super::*;
use crate::channel_mode::ChannelMode;

impl Channels {
    /// The sending channel `channel`.
    pub(crate) fn sender_mut(&mut self, channel: ChannelTypeId) -> Option<&mut ChannelSender> {
        self.senders
            .iter_mut()
            .find(|sender| sender.channel_type_id() == channel)
    }

    /// Sets the bytes the receive side may hold for messages it can not deliver yet.
    fn with_receive_budget(mut self, bytes: usize) -> Self {
        self.receive_budget = bytes;
        self
    }
}

/// The senders' fragment size follows the largest payload the packet session hands over: sizing for
/// 256 bytes, then linking at 200, gives each one's size; a link at 0 keeps the last.
#[test]
fn the_fragment_len_follows_the_packet_sessions_largest_payload() {
    let mut channels = Channels::default();
    channels.size_fragments(256);
    assert_eq!(
        channels.fragment_len,
        fragment_len_for_max_payload(256).unwrap()
    );
    channels.link(200);
    assert_eq!(
        channels.fragment_len,
        fragment_len_for_max_payload(200).unwrap()
    );
    channels.link(0);
    assert_eq!(
        channels.fragment_len,
        fragment_len_for_max_payload(200).unwrap()
    );
}

/// A payload too small for one byte of fragment holds no fragment.
#[test]
#[should_panic(expected = "holds no fragment")]
fn a_payload_below_a_fragments_overhead_fails_the_link() {
    Channels::default().link(MIN_FRAGMENT_PAYLOAD - 1);
}

/// A link whose fragments can not carry a sending channel's largest message in `MAX_FRAGMENTS`
/// fails: at a 200-byte payload, a channel of 16,383 fragments and a byte more.
#[test]
#[should_panic(expected = "passes 16383 fragments")]
fn a_link_whose_fragments_can_not_carry_a_channels_message_fails() {
    let mut channels = Channels::default();
    let setup = ChannelSetup {
        max_message_len: MAX_FRAGMENTS * fragment_len_for_max_payload(200).unwrap() + 1,
        ..ChannelSetup::default()
    };
    channels.add_channel_send::<ResetChannel>(setup, NetId::new(0));
    channels.link(200);
}

/// A reset drops every pending message, and each channel keeps its own setup, so a reset needs
/// no registry.
#[test]
fn a_reset_drops_the_messages_and_keeps_each_channels_setup() {
    let mut channels = Channels::default();
    let channel = ChannelSetup {
        mode: ChannelMode::OrderedReliable,
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
    assert!(sender.watches_acks());
}

#[derive(Debug)]
struct ResetChannel;
impl Channel for ResetChannel {}

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

    let too_large = Err(SendError::MessageTooLarge {
        len: 11,
        max_len: 10,
    });
    assert_eq!(channels.send(id, Bytes::from(vec![0; 10]), 1.0), Ok(()));
    assert_eq!(channels.send(id, Bytes::from(vec![0; 11]), 1.0), too_large);
    assert_eq!(channels.staged.len(), 1);
    assert!(channels.send_mut(id, Bytes::from(vec![0; 10]), 1.0).is_ok());
    assert_eq!(
        channels.send_mut(id, Bytes::from(vec![0; 11]), 1.0),
        too_large.map(|()| None)
    );
    // A channel the transport does not send on is refused too.
    assert_eq!(
        channels.send(NetId::new(1), Bytes::from(vec![0; 1]), 1.0),
        Err(SendError::ChannelNotFound(NetId::new(1)))
    );
}

/// The steps' tests: what a flush gives a packet session's queues, and what a receive takes from
/// them.
mod steps {
    use super::*;
    use crate::channel_message::ChannelMessage;
    use crate::fragment_count::FragmentCount;
    use crate::fragment_index::FragmentIndex;
    use crate::packet::DEFAULT_MAX_PAYLOAD;
    use crate::tick_encoder::LONG_TICK_MARK;
    use bytes::BytesMut;
    use campfire_net_bytes::SerializationError;
    use campfire_net_payload::{PacketNumber, ReceivePayload};

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
    ) -> Result<Option<TickInstant>, ReceiveError> {
        channels.receive(queues, Instant::ZERO, Duration::ZERO)
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

    /// A data packet whose `body` follows the long tick 1, at no overstep.
    fn data_payload(body: &[u8]) -> Vec<u8> {
        let mut payload = vec![LONG_TICK_MARK, 0, 0, 0, 1, 0];
        payload.extend_from_slice(body);
        payload
    }

    /// A flush that the packet session's budget stops after one packet keeps the second message of
    /// a channel that retries unsent messages, and drops it on one that does not. One packet of
    /// 1,000 bytes of message takes 1,010: a long tick of 5, the overstep, the channel id, the
    /// count, a length of 2 and the message. A budget of 1,100 leaves 90 after it.
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
            channels.flush(&mut queues, TickInstant::ZERO, Instant::ZERO);
            let sends: Vec<_> = queues.take_sends().collect();
            assert_eq!(sends.len(), 1);
            assert_eq!(sends[0].data().len(), 1_010);
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
            channels.flush(queues, TickInstant::ZERO, Instant::ZERO);
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
        assert_eq!(
            channels.tick_encoder.encode(TickInstant::ZERO).as_bytes(),
            [0, 0, 0]
        );
    }

    /// A link queues 1 MiB at most: two messages of 512 KiB fill it, and one more byte is refused
    /// with what is queued. An unreliable channel's flush sends them, and the link takes the
    /// byte; a reliable channel holds its messages until each of their packets is delivered.
    #[test]
    fn a_link_queues_its_send_budget_at_most() {
        for (mode, held) in [
            (ChannelMode::UnorderedUnreliable, false),
            (ChannelMode::OrderedReliable, true),
        ] {
            let setup = ChannelSetup {
                mode,
                ..Default::default()
            };
            let mut channels = Channels::default();
            channels.add_channel_send::<FateChannel>(setup, NetId::new(0));
            let half = Bytes::from(vec![0; SEND_BUDGET / 2]);
            for _ in 0..2 {
                channels.send(NetId::new(0), half.clone(), 1.0).unwrap();
            }
            assert_eq!(
                channels.send(NetId::new(0), Bytes::from_static(b"b"), 1.0),
                Err(SendError::Backlogged {
                    len: 1,
                    queued: SEND_BUDGET,
                })
            );
            let mut queues = queues(usize::MAX);
            channels.flush(&mut queues, TickInstant::ZERO, Instant::ZERO);
            let tokens = queues
                .take_sends()
                .map(|send| send.token)
                .collect::<Vec<_>>();
            assert_eq!(
                channels.queued(),
                if held { SEND_BUDGET } else { 0 },
                "{mode:?}"
            );
            for token in tokens {
                queues.push_fate(PayloadFate::Delivered(token));
            }
            receive(&mut channels, &mut queues).unwrap();
            assert_eq!(channels.queued(), 0, "{mode:?}");
            assert_eq!(
                channels.send(NetId::new(0), Bytes::from_static(b"b"), 1.0),
                Ok(())
            );
        }
    }

    /// An idle transport sends its tick: a flush with no message sends a packet of the instant
    /// alone, a long tick of 5 bytes and the overstep, when 100 ms passed since its last packet, and
    /// none before. A message sent in between counts as a packet: the instant, the channel id, the
    /// count, the length and the byte, 6 + 1 + 1 + 1 + 1.
    #[test]
    fn an_idle_transport_sends_its_tick() {
        let mut channels = Channels::default();
        channels.add_channel_send::<FateChannel>(ChannelSetup::default(), NetId::new(0));
        let mut queues = queues(usize::MAX);
        let mut flush_at = |channels: &mut Channels, millis| {
            channels.flush(&mut queues, TickInstant::ZERO, at(millis));
            queues
                .take_sends()
                .map(|send| send.data().len())
                .collect::<Vec<_>>()
        };
        assert_eq!(flush_at(&mut channels, 0), [6]);
        assert_eq!(flush_at(&mut channels, 99), Vec::<usize>::new());
        assert_eq!(flush_at(&mut channels, 100), [6]);
        channels
            .send(NetId::new(0), Bytes::from_static(b"m"), 1.0)
            .unwrap();
        assert_eq!(flush_at(&mut channels, 150), [10]);
        assert_eq!(flush_at(&mut channels, 249), Vec::<usize>::new());
        assert_eq!(flush_at(&mut channels, 250), [6]);
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
            sender.link(sender_payload);
            let mut receiver = Channels::default();
            receiver.add_channel_receive::<SmallPayloadChannel>(setup, channel_id);
            receiver.link(receiver_payload);
            let mut sender_queues = queues(usize::MAX);
            sender_queues.set_max_payload(sender_payload);
            let mut receiver_queues = queues(usize::MAX);
            receiver_queues.set_max_payload(receiver_payload);

            let expected = Bytes::from(vec![5; 3000]);
            sender.send(channel_id, expected.clone(), 1.0).unwrap();
            sender.flush(&mut sender_queues, TickInstant::ZERO, Instant::ZERO);
            let sends: Vec<_> = sender_queues.take_sends().collect();
            // A payload of 1,166 bytes takes fragments of 1,148, so 3,000 = 2 · 1,148 + 704 go in
            // 3; one of 256, of 238, so 3,000 = 12 · 238 + 144 go in 13.
            let fragment_len = fragment_len_for_max_payload(sender_payload).unwrap();
            assert_eq!(
                (fragment_len, sends.len()),
                if sender_payload == 256 {
                    (238, 13)
                } else {
                    (1148, 3)
                }
            );
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
    /// before it, the two payload kinds past the fragment's, a batch of no message and an empty
    /// final fragment each break the protocol: the receive returns the error, admits nothing and
    /// accepts nothing, so the packet session never acknowledges the packet, and the packet
    /// queued behind it is not read. 0xff starts an 8-byte varint channel id with no bytes after
    /// it; channel 5 carries one message with no id, a length of 1 and the byte 7.
    #[test]
    fn a_protocol_violation_admits_and_accepts_nothing() {
        let kind = |bits| OpenedPayload {
            kind: PayloadKind::new(bits),
            ..opened(0, PacketKind::Data, &data_payload(&[]))
        };
        let mut empty_final = vec![0];
        FragmentBody {
            message_id: MessageId(0),
            fragment_index: FragmentIndex(1),
            fragments: FragmentCount(2),
            bytes: Bytes::new(),
        }
        .to_bytes(&mut empty_final);
        let violations = [
            (
                opened(0, PacketKind::Data, &data_payload(&[0xff])),
                ReceiveError::Serialization(SerializationError::Truncated),
            ),
            (
                opened(0, PacketKind::Data, &data_payload(&[5, 1, 1, 7])),
                ReceiveError::UnknownChannel(NetId::new(5)),
            ),
            (
                opened(0, PacketKind::Data, &[0, 1]),
                ReceiveError::MalformedTick,
            ),
            (kind(2), ReceiveError::UnknownPacketKind(2)),
            (kind(3), ReceiveError::UnknownPacketKind(3)),
            (
                opened(0, PacketKind::Data, &data_payload(&[0, 0])),
                ReceiveError::EmptyBatch,
            ),
            (
                opened(0, PacketKind::DataFragment, &data_payload(&empty_final)),
                ReceiveError::EmptyFragment,
            ),
        ];
        for (violation, expected) in violations {
            let mut channels = Channels::default();
            channels.add_channel_receive::<FateChannel>(ChannelSetup::default(), NetId::new(0));
            let mut queues = queues(usize::MAX);
            queues.push_opened(violation);
            queues.push_opened(opened(1, PacketKind::Data, &data_payload(&[])));
            let received = channels.receive(&mut queues, Instant::ZERO, Duration::ZERO);
            assert_eq!(received, Err(expected));
            assert_eq!(queues.take_accepted().count(), 0, "{expected:?}");
            assert_eq!(queues.opened().count(), 0, "{expected:?}");
            assert_eq!(channels.backlog().partial_fragments, 0, "{expected:?}");
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

    /// An ordered channel of 100-byte messages under a budget of two store slots and 200 bytes:
    /// messages 1 and 2 fill it behind the gap of message 0, so the packet with message 3, whose
    /// store would grow by two slots, is not accepted, and gets no ACK. Once message 0 arrives
    /// and the three are read, the same packet is admitted. Of the four payloads the frame read,
    /// it accepts the three it admitted.
    #[test]
    fn a_packet_over_the_receive_budget_is_not_accepted() {
        let setup = ChannelSetup {
            mode: ChannelMode::OrderedReliable,
            max_message_len: 100,
            ..Default::default()
        };
        let mut channels =
            Channels::default().with_receive_budget(2 * size_of::<ChannelMessage>() + 200);
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
        sender.flush(&mut queues, TickInstant::ZERO, Instant::ZERO);
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
        // Of the seed's 3 · 2,048 = 6,144 mutations, 2,989 read and 3,155 break.
        assert_eq!([read, broken], [2989, 3155]);
    }
}
