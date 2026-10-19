use super::*;
use crate::channel::Channel;

use crate::compression_scratch::CompressionScratch;
use crate::compression_setup::CompressionSetup;
use crate::fragmented_message::FragmentedMessage;
use crate::packet::FRAGMENT_LEN;

use crate::single_body::SingleBody;

#[derive(Debug)]
struct TestChannel;
impl Channel for TestChannel {}

fn message(id: Option<u16>, tick: u32, bytes: &'static [u8]) -> ReceiveMessage {
    ReceiveMessage {
        data: SingleBody::new(id.map(MessageId), Bytes::from_static(bytes)).into(),
        remote_sent_tick: NetTick(tick),
        compression: CompressionSetup::DISABLED,
    }
}

/// Receives with room to spare.
fn receive(channel: &mut ChannelReceiver, message: ReceiveMessage) -> Result<BufferOutcome> {
    channel.buffer_receive(message, &mut ReceiveBudget::new(usize::MAX))
}

fn channel(mode: ChannelMode) -> ChannelReceiver {
    ChannelReceiver::new(
        ChannelTypeId::of::<TestChannel>(),
        "TestChannel",
        &ChannelSetup {
            mode,
            ..ChannelSetup::default()
        },
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
    // The ring drops each delivered message: it starts again at the next id, empty.
    let ReceiveState::ReliableOrdered { pending, ready } = &channel.state else {
        unreachable!("the channel was built in `ReliableOrdered` mode")
    };
    assert_eq!((*pending, ready.len()), (MessageId(1), 0));
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
    // Message 1 is past the gap at 0, where the ring still marks it received.
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
    assert!(received.is_empty(), "the ring drops the received prefix");

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
fn fragments(id: u16) -> [ReceiveMessage; 2] {
    let bytes = Bytes::from(vec![id as u8; 2 * FRAGMENT_LEN - 1]);
    let [first, second] = <[_; 2]>::try_from(
        FragmentedMessage::split(
            MessageId(id),
            bytes,
            FRAGMENT_LEN,
            CompressionSetup::DISABLED,
            &mut CompressionScratch::default(),
        )
        .fragment_body(),
    )
    .unwrap();
    [first, second].map(|fragment| ReceiveMessage {
        data: fragment.into(),
        remote_sent_tick: NetTick(u32::from(id)),
        compression: CompressionSetup::DISABLED,
    })
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

/// With 10 bytes of room on an ordered channel, message 1 waits behind the gap of message 0 and
/// takes 6; message 2's 5 bytes do not fit and are refused unstored; the head, message 0, is
/// kept past the room; reading 0 and 1 releases every held byte.
#[test]
fn ordered_messages_behind_a_gap_hold_budget() {
    let mut channel = channel(ChannelMode::OrderedReliable);
    let mut budget = ReceiveBudget::new(10);
    let behind = |id, bytes| message(Some(id), 0, bytes);
    assert_eq!(
        channel
            .buffer_receive(behind(1, b"sixsix"), &mut budget)
            .unwrap(),
        BufferOutcome::Stored
    );
    assert_eq!(
        channel
            .buffer_receive(behind(2, b"fives"), &mut budget)
            .unwrap(),
        BufferOutcome::OverBudget
    );
    assert_eq!(channel.held(), 6);
    assert_eq!(
        channel
            .buffer_receive(behind(0, b"the head is twenty b"), &mut budget)
            .unwrap(),
        BufferOutcome::Stored
    );
    assert_eq!(channel.held(), 26);
    assert_eq!(
        channel.read_message().unwrap().message_id,
        Some(MessageId(0))
    );
    assert_eq!(
        channel.read_message().unwrap().message_id,
        Some(MessageId(1))
    );
    assert_eq!(channel.read_message(), None, "message 2 was never stored");
    assert_eq!(channel.held(), 0);
}

/// A reassembly buffer of 2 * FRAGMENT_SIZE bytes does not fit FRAGMENT_SIZE bytes of room on an
/// unordered channel: the fragment is refused and leaves no partial. The ordered head is
/// reserved past the room, and its completed message releases the reservation.
#[test]
fn reassembly_reserves_budget_and_the_head_is_reserved_anyway() {
    let mut unordered = channel(ChannelMode::UnorderedReliable);
    let [first, _] = fragments(0);
    let mut budget = ReceiveBudget::new(FRAGMENT_LEN);
    assert_eq!(
        unordered.buffer_receive(first, &mut budget).unwrap(),
        BufferOutcome::OverBudget
    );
    assert_eq!((unordered.partial_fragments(), unordered.held()), (0, 0));

    let mut ordered = channel(ChannelMode::OrderedReliable);
    let [first, second] = fragments(0);
    let mut budget = ReceiveBudget::new(FRAGMENT_LEN);
    assert_eq!(
        ordered.buffer_receive(first, &mut budget).unwrap(),
        BufferOutcome::Stored
    );
    assert_eq!(ordered.held(), 2 * FRAGMENT_LEN);
    assert_eq!(
        ordered.buffer_receive(second, &mut budget).unwrap(),
        BufferOutcome::Stored
    );
    // The completed message waits for the read, holding its own length only.
    assert_eq!(ordered.held(), 2 * FRAGMENT_LEN - 1);
    assert!(ordered.read_message().is_some());
    assert_eq!(ordered.held(), 0);
}
