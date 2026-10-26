use super::*;
use crate::channel::Channel;
use crate::fragment_receiver::PARTIAL_COST;
use crate::fragmented_message::FragmentedMessage;
use crate::packet::FRAGMENT_LEN;

#[derive(Debug)]
struct TestChannel;
impl Channel for TestChannel {}

/// A message or a fragment as a packet carries it, with the packet's tick.
#[derive(Debug)]
enum Part {
    Single(SingleBody, NetTick),
    Fragment(FragmentBody, NetTick),
}

impl ChannelReceiver {
    /// Stores `part`, as a packet's read does.
    fn receive_part(
        &mut self,
        part: Part,
        budget: &mut ReceiveBudget,
    ) -> Result<BufferOutcome, ReceiveError> {
        match part {
            Part::Single(single, tick) => self.receive_single(single, tick, budget),
            Part::Fragment(fragment, tick) => self.receive_fragment(fragment, tick, budget),
        }
    }
}

fn message(id: Option<u16>, tick: u32, bytes: &'static [u8]) -> Part {
    Part::Single(
        SingleBody::new(id.map(MessageId), Bytes::from_static(bytes)),
        NetTick(tick),
    )
}

/// Receives with room to spare.
fn receive(channel: &mut ChannelReceiver, part: Part) -> Result<BufferOutcome, ReceiveError> {
    channel.receive_part(part, &mut ReceiveBudget::new(usize::MAX))
}

fn channel(mode: ChannelMode) -> ChannelReceiver {
    ChannelReceiver::new(
        ChannelTypeId::of::<TestChannel>(),
        "TestChannel",
        &ChannelSetup {
            mode,
            ..ChannelSetup::default()
        },
        ChannelReader::Messages,
    )
}

#[test]
fn every_channel_mode_constructs_one_channel_receive() {
    for mode in [
        ChannelMode::UnorderedUnreliable,
        ChannelMode::SequencedUnreliable,
        ChannelMode::UnorderedReliable,
        ChannelMode::OrderedReliable,
        ChannelMode::SequencedReliable,
    ] {
        assert_eq!(
            channel(mode).channel_type_id(),
            ChannelTypeId::of::<TestChannel>()
        );
    }
}

#[test]
fn ordered_reliable_waits_for_the_missing_sequence() {
    let mut channel = channel(ChannelMode::OrderedReliable);
    receive(&mut channel, message(Some(1), 1, b"one")).unwrap();
    assert_eq!(channel.read_message(), None);
    receive(&mut channel, message(Some(0), 0, b"zero")).unwrap();
    assert_eq!(
        channel.read_message().unwrap().message_id,
        Some(MessageId(0))
    );
    assert_eq!(
        channel.read_message().unwrap().message_id,
        Some(MessageId(1))
    );
}

#[test]
fn an_ordered_reliable_channel_buffers_across_the_wrap_of_the_message_id() {
    let mut channel = channel(ChannelMode::OrderedReliable);
    let ReceiveState::ReliableOrdered { pending, .. } = &mut channel.state else {
        unreachable!("the channel was built in `ReliableOrdered` mode")
    };
    *pending = MessageId(u16::MAX);

    receive(&mut channel, message(Some(0), 0, b"zero")).unwrap();
    assert_eq!(channel.read_message(), None);
    receive(&mut channel, message(Some(u16::MAX), 1, b"max")).unwrap();

    assert_eq!(
        channel.read_message().unwrap().message_id,
        Some(MessageId(u16::MAX))
    );
    assert_eq!(
        channel.read_message().unwrap().message_id,
        Some(MessageId(0))
    );
    // The store gives back each delivered message: it waits for the next id, empty.
    let ReceiveState::ReliableOrdered {
        pending,
        ready,
        waiting,
    } = &channel.state
    else {
        unreachable!("the channel was built in `ReliableOrdered` mode")
    };
    assert_eq!(
        (*pending, ready.len(), waiting.held()),
        (MessageId(1), 0, 0)
    );
}

/// A message 32,767 ids past the one an ordered channel waits for holds its own entry and a copy
/// of its bytes, not a slot for each id between, nor the packet its bytes came in; taken out, it
/// leaves the channel holding nothing.
#[test]
fn a_message_far_past_the_gap_holds_its_entry_and_a_copy_of_its_bytes() {
    // The farthest id an honest sender sends, 16,383 past the one this end waits for, holds one
    // entry; one id farther, in either reliable mode, breaks the protocol.
    let mut unordered = channel(ChannelMode::UnorderedReliable);
    let mut channel = channel(ChannelMode::OrderedReliable);
    let packet = Bytes::from(vec![7; 64]);
    let far = |id| {
        Part::Single(
            SingleBody::new(Some(MessageId(id)), packet.slice(10..14)),
            NetTick(0),
        )
    };
    receive(&mut channel, far(16_383)).unwrap();
    assert_eq!(channel.held(), GapStore::<ChannelMessage>::SLOT + 4);
    assert_eq!(
        receive(&mut channel, far(16_384)),
        Err(ReceiveError::PastWindow)
    );
    assert_eq!(
        receive(&mut unordered, far(16_384)),
        Err(ReceiveError::PastWindow)
    );
    let ReceiveState::ReliableOrdered { waiting, .. } = &mut channel.state else {
        unreachable!("the channel was built in `ReliableOrdered` mode")
    };
    let held = waiting.pop(MessageId(16_383)).unwrap();
    assert!(!packet.as_ptr_range().contains(&held.bytes.as_ptr()));
    assert_eq!(held.bytes, packet.slice(10..14));
    assert_eq!(channel.held(), 0);
}

#[test]
fn an_unordered_unreliable_channel_exposes_no_message_id() {
    let mut channel = channel(ChannelMode::UnorderedUnreliable);
    receive(&mut channel, message(Some(42), 3, b"payload")).unwrap();
    assert_eq!(channel.read_message().unwrap().message_id, None);
}

#[test]
fn sequenced_unreliable_rejects_messages_older_than_the_latest_seen() {
    let mut channel = channel(ChannelMode::SequencedUnreliable);
    receive(&mut channel, message(Some(2), 2, b"newest")).unwrap();
    receive(&mut channel, message(Some(1), 1, b"stale")).unwrap();
    let received = channel.read_message().unwrap();
    assert_eq!(received.bytes, Bytes::from_static(b"newest"));
    assert_eq!(received.message_id, Some(MessageId(2)));
    assert_eq!(channel.read_message(), None);
}

#[test]
fn a_sequenced_unreliable_channel_replaces_its_buffered_message_and_refuses_a_copy() {
    let mut channel = channel(ChannelMode::SequencedUnreliable);
    receive(&mut channel, message(Some(0), 0, b"zero")).unwrap();
    receive(&mut channel, message(Some(1), 1, b"one")).unwrap();
    receive(&mut channel, message(Some(2), 2, b"two")).unwrap();
    receive(&mut channel, message(Some(2), 3, b"duplicate")).unwrap();

    let received = channel.read_message().unwrap();
    assert_eq!(received.bytes, Bytes::from_static(b"two"));
    assert_eq!(received.message_id, Some(MessageId(2)));
    assert_eq!(channel.read_message(), None);

    receive(&mut channel, message(Some(2), 4, b"late duplicate")).unwrap();
    assert_eq!(channel.read_message(), None);
}

#[test]
fn an_unordered_reliable_channel_advances_past_messages_that_arrive_out_of_order() {
    let mut channel = channel(ChannelMode::UnorderedReliable);
    receive(&mut channel, message(Some(1), 1, b"one")).unwrap();
    assert_eq!(
        channel.read_message().unwrap().message_id,
        Some(MessageId(1))
    );
    // Message 1 is past the gap at 0, where the store still marks it received.
    receive(
        &mut channel,
        message(Some(1), 1, b"duplicate before the gap fills"),
    )
    .unwrap();
    assert_eq!(channel.read_message(), None);
    receive(&mut channel, message(Some(0), 0, b"zero")).unwrap();
    assert_eq!(
        channel.read_message().unwrap().message_id,
        Some(MessageId(0))
    );
    receive(&mut channel, message(Some(1), 2, b"duplicate")).unwrap();
    assert_eq!(channel.read_message(), None);
}

#[test]
fn an_unordered_reliable_channel_advances_and_drops_copies_across_the_wrap_of_the_message_id() {
    let mut channel = channel(ChannelMode::UnorderedReliable);
    let ReceiveState::ReliableUnordered { pending, .. } = &mut channel.state else {
        unreachable!("the channel was built in `ReliableUnordered` mode")
    };
    *pending = MessageId(u16::MAX);

    receive(&mut channel, message(Some(0), 0, b"zero")).unwrap();
    receive(&mut channel, message(Some(u16::MAX), 1, b"max")).unwrap();

    assert_eq!(
        channel.read_message().unwrap().message_id,
        Some(MessageId(0))
    );
    assert_eq!(
        channel.read_message().unwrap().message_id,
        Some(MessageId(u16::MAX))
    );
    let ReceiveState::ReliableUnordered { pending, .. } = &channel.state else {
        unreachable!("the channel was built in `ReliableUnordered` mode")
    };
    assert_eq!(*pending, MessageId(1));
    let ReceiveState::ReliableUnordered { received, .. } = &channel.state else {
        unreachable!("the channel was built in `ReliableUnordered` mode")
    };
    assert_eq!(received.held(), 0, "the store drops the received prefix");

    receive(&mut channel, message(Some(u16::MAX), 2, b"duplicate max")).unwrap();
    receive(&mut channel, message(Some(0), 3, b"duplicate zero")).unwrap();
    assert_eq!(channel.read_message(), None);
}

#[test]
fn sequenced_reliable_exposes_only_the_newest_buffered_message() {
    let mut channel = channel(ChannelMode::SequencedReliable);
    receive(&mut channel, message(Some(0), 0, b"zero")).unwrap();
    receive(&mut channel, message(Some(1), 1, b"one")).unwrap();
    let received = channel.read_message().unwrap();
    assert_eq!(received.bytes, Bytes::from_static(b"one"));
    assert_eq!(received.message_id, Some(MessageId(1)));
    assert_eq!(channel.read_message(), None);
}

#[test]
fn a_sequenced_reliable_channel_delivers_no_message_again_after_a_drain() {
    let mut channel = channel(ChannelMode::SequencedReliable);
    receive(&mut channel, message(Some(1), 1, b"one")).unwrap();
    assert_eq!(
        channel.read_message().unwrap().message_id,
        Some(MessageId(1))
    );

    receive(&mut channel, message(Some(1), 2, b"duplicate")).unwrap();
    assert_eq!(channel.read_message(), None);
}

#[test]
fn sequenced_reliable_accepts_the_newest_message_across_message_id_rollover() {
    let mut channel = channel(ChannelMode::SequencedReliable);
    let ReceiveState::Sequenced {
        newest_seen,
        newest_completed,
        ..
    } = &mut channel.state
    else {
        unreachable!("the channel was built in `ReliableSequenced` mode")
    };
    *newest_seen = Some(MessageId(u16::MAX));
    *newest_completed = Some(MessageId(u16::MAX));

    receive(&mut channel, message(Some(0), 1, b"after wrap")).unwrap();

    let received = channel.read_message().unwrap();
    assert_eq!(received.bytes, Bytes::from_static(b"after wrap"));
    assert_eq!(received.message_id, Some(MessageId(0)));
}

/// The two fragments of message `id`, of 2 * FRAGMENT_SIZE - 1 bytes.
fn fragments(id: u16) -> [Part; 2] {
    let bytes = Bytes::from(vec![id as u8; 2 * FRAGMENT_LEN - 1]);
    let [first, second] = <[_; 2]>::try_from(
        FragmentedMessage::split(MessageId(id), bytes, FRAGMENT_LEN).fragment_body(),
    )
    .unwrap();
    [first, second].map(|fragment| Part::Fragment(fragment, NetTick(u32::from(id))))
}

/// In both sequenced modes, message 2 supersedes the half-received message 1, whose partial is
/// dropped; a late copy of a fragment of the completed message 2 opens no partial either.
#[test]
fn a_sequenced_channel_drops_a_superseded_partial_and_a_copy_of_a_completed_one() {
    for mode in [
        ChannelMode::SequencedUnreliable,
        ChannelMode::SequencedReliable,
    ] {
        let mut channel = channel(mode);
        let [first_of_1, _] = fragments(1);
        receive(&mut channel, first_of_1).unwrap();
        assert_eq!(channel.partial_fragments(), 1, "{mode:?}");

        let [first_of_2, second_of_2] = fragments(2);
        receive(&mut channel, first_of_2).unwrap();
        assert_eq!(
            channel.partial_fragments(),
            1,
            "{mode:?}: only message 2 is partial"
        );
        receive(&mut channel, second_of_2).unwrap();
        assert_eq!(channel.partial_fragments(), 0, "{mode:?}");
        assert_eq!(
            channel.read_message().unwrap().message_id,
            Some(MessageId(2))
        );

        let [late_copy, _] = fragments(2);
        receive(&mut channel, late_copy).unwrap();
        assert_eq!(channel.partial_fragments(), 0, "{mode:?}");
        assert_eq!(channel.read_message(), None, "{mode:?}");
    }
}

/// After 3 s of silence an unreliable unordered partial expires; a reliable one does not, since
/// its acked fragments are never resent, and completes when its last fragment arrives.
#[test]
fn only_the_partials_of_an_unreliable_unordered_channel_expire() {
    for (mode, expires) in [
        (ChannelMode::UnorderedUnreliable, true),
        (ChannelMode::OrderedReliable, false),
        (ChannelMode::UnorderedReliable, false),
    ] {
        let mut channel = channel(mode);
        let [first, second] = fragments(0);
        channel.update(Instant::ZERO);
        receive(&mut channel, first).unwrap();
        channel.update(Instant::from_elapsed(Duration::from_secs(4)));
        assert_eq!(
            channel.partial_fragments(),
            usize::from(!expires),
            "{mode:?}"
        );
        receive(&mut channel, second).unwrap();
        assert_eq!(channel.read_message().is_some(), !expires, "{mode:?}");
    }
}

/// A whole message of the id of a partial one delivers once, behind a gap or at its head, on both
/// reliable modes that keep every message, and the partial goes: its other fragment is a copy.
#[test]
fn a_whole_and_a_fragmented_message_of_one_id_deliver_once() {
    for mode in [ChannelMode::UnorderedReliable, ChannelMode::OrderedReliable] {
        for id in [0, 1] {
            let mut channel = channel(mode);
            let [first, second] = fragments(id);
            receive(&mut channel, first).unwrap();
            receive(&mut channel, message(Some(id), 0, b"whole")).unwrap();
            receive(&mut channel, second).unwrap();
            if id == 1 {
                receive(&mut channel, message(Some(0), 0, b"head")).unwrap();
            }
            let delivered = core::iter::from_fn(|| channel.read_message())
                .filter(|message| message.message_id == Some(MessageId(id)))
                .map(|message| message.bytes)
                .collect::<Vec<_>>();
            assert_eq!(delivered, [Bytes::from_static(b"whole")], "{mode:?} {id}");
            assert_eq!(channel.partial_fragments(), 0, "{mode:?} {id}");
        }
    }
}

/// An entry behind a gap costs its bytes and its slot. With room for two slots and 10 bytes on an
/// ordered channel, message 1 waits behind the gap of message 0 and takes a slot and its 6 bytes;
/// message 2's slot and 5 bytes do not fit the slot and 4 bytes left and are refused unstored; the
/// head, message 0, is delivered past the room, with message 1 behind it, and the channel holds
/// nothing.
#[test]
fn ordered_messages_behind_a_gap_hold_budget() {
    let slot = GapStore::<ChannelMessage>::SLOT;
    let mut channel = channel(ChannelMode::OrderedReliable);
    let mut budget = ReceiveBudget::new(2 * slot + 10);
    let behind = |id, bytes| message(Some(id), 0, bytes);
    assert_eq!(
        channel
            .receive_part(behind(1, b"sixsix"), &mut budget)
            .unwrap(),
        BufferOutcome::Stored
    );
    assert_eq!(
        channel
            .receive_part(behind(2, b"fives"), &mut budget)
            .unwrap(),
        BufferOutcome::OverBudget
    );
    assert_eq!(channel.held(), slot + 6);
    assert_eq!(
        channel
            .receive_part(behind(0, b"the head"), &mut budget)
            .unwrap(),
        BufferOutcome::Stored
    );
    assert_eq!(channel.held(), 0);
    assert_eq!(
        channel.read_message().unwrap().message_id,
        Some(MessageId(0))
    );
    assert_eq!(
        channel.read_message().unwrap().message_id,
        Some(MessageId(1))
    );
    assert_eq!(channel.read_message(), None, "message 2 was never stored");
}

/// A partial of two fragments of `FRAGMENT_LEN` costs its map entry, its reassembly buffer of
/// 2 · FRAGMENT_LEN bytes and its word of receipts: FRAGMENT_LEN bytes of room do not hold it on
/// an unordered channel, which refuses the fragment and keeps no partial. The ordered head is
/// reserved past the room, and its completed message is delivered and holds nothing. Message 2,
/// behind the gap of message 1, holds its partial, then its entry and its 2 · FRAGMENT_LEN - 1
/// bytes.
#[test]
fn reassembly_reserves_budget_and_the_head_is_reserved_anyway() {
    let partial = PARTIAL_COST + 2 * FRAGMENT_LEN + size_of::<u64>();
    let mut unordered = channel(ChannelMode::UnorderedReliable);
    let [first, _] = fragments(0);
    let mut budget = ReceiveBudget::new(FRAGMENT_LEN);
    assert_eq!(
        unordered.receive_part(first, &mut budget).unwrap(),
        BufferOutcome::OverBudget
    );
    assert_eq!((unordered.partial_fragments(), unordered.held()), (0, 0));

    let mut ordered = channel(ChannelMode::OrderedReliable);
    let [first, second] = fragments(0);
    let mut budget = ReceiveBudget::new(FRAGMENT_LEN);
    assert_eq!(
        ordered.receive_part(first, &mut budget).unwrap(),
        BufferOutcome::Stored
    );
    assert_eq!(ordered.held(), partial);
    assert_eq!(
        ordered.receive_part(second, &mut budget).unwrap(),
        BufferOutcome::Stored
    );
    assert_eq!(ordered.held(), 0);
    assert!(ordered.read_message().is_some());

    let [first, second] = fragments(2);
    let mut budget = ReceiveBudget::new(usize::MAX);
    ordered.receive_part(first, &mut budget).unwrap();
    assert_eq!(ordered.held(), partial);
    ordered.receive_part(second, &mut budget).unwrap();
    assert_eq!(
        ordered.held(),
        GapStore::<ChannelMessage>::SLOT + 2 * FRAGMENT_LEN - 1
    );
}
