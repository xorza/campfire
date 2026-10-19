use super::*;
use crate::channel::Channel;
use crate::channel_config::DEFAULT_MAX_MESSAGE_SIZE;

use crate::packet::FRAGMENT_SIZE;

#[derive(Debug)]
struct TestChannel;
impl Channel for TestChannel {}

impl ChannelSender {
    /// Buffers `message` with a fresh compression scratch.
    fn buffer(
        &mut self,
        message: Bytes,
        priority: f32,
        compression: CompressionConfig,
    ) -> Option<MessageId> {
        self.buffer_send(
            message,
            priority,
            FRAGMENT_SIZE,
            compression,
            &mut CompressionScratch::default(),
        )
    }
}

fn channel(mode: ChannelMode, retry_unsent_messages: bool) -> ChannelSender {
    ChannelSender::new(
        ChannelKind::of::<TestChannel>(),
        NetId::new(0),
        core::any::type_name::<TestChannel>(),
        &ChannelConfig {
            mode,
            retry_unsent_messages,
            ..ChannelConfig::default()
        },
    )
}

const fn fragment_ack(fragment_index: u16) -> SendMessageKey {
    SendMessageKey::ReliableFragment(MessageId(0), fragment_index)
}

#[test]
fn every_channel_mode_constructs_one_channel_send() {
    for (mode, watches_acks) in [
        (ChannelMode::UnorderedUnreliable, false),
        (ChannelMode::SequencedUnreliable, false),
        (ChannelMode::UnorderedReliable, true),
        (ChannelMode::OrderedReliable, true),
        (ChannelMode::SequencedReliable, true),
    ] {
        let channel = channel(mode, true);
        assert_eq!(channel.mode(), mode);
        assert_eq!(channel.watches_acks(), watches_acks);
    }
}

#[test]
fn unreliable_candidates_remain_pending_until_commit() {
    let mut channel = channel(ChannelMode::UnorderedUnreliable, true);
    channel.buffer(
        Bytes::from_static(b"pending"),
        1.0,
        CompressionConfig::DISABLED,
    );
    let mut candidates = Vec::new();
    channel.collect_send_candidates(&mut candidates);
    assert_eq!(candidates.len(), 1);
    channel.finish_send(SendFlushOutcome::BudgetLimited);
    candidates.clear();
    channel.collect_send_candidates(&mut candidates);
    assert_eq!(candidates.len(), 1);
    channel.commit_send(candidates[0].key);
    channel.finish_send(SendFlushOutcome::Complete);
    candidates.clear();
    channel.collect_send_candidates(&mut candidates);
    assert!(candidates.is_empty());
}

/// A candidate names its message by key and carries the length and rank its order needs. The
/// single `pending` of an unordered unreliable channel is 8 bytes on the wire: 1 for its length
/// and 7 of payload, and no id. A message one byte past the fragment size splits into two
/// fragments, which are one candidate: it names fragment 0, and fragment 1 follows it.
#[test]
fn candidates_name_their_message_and_carry_its_length_and_rank() {
    let mut channel = channel(ChannelMode::UnorderedUnreliable, true);
    channel.buffer(
        Bytes::from_static(b"pending"),
        2.0,
        CompressionConfig::DISABLED,
    );
    channel.buffer(
        Bytes::from(vec![1; FRAGMENT_SIZE + 1]),
        1.0,
        CompressionConfig::DISABLED,
    );
    let mut candidates = Vec::new();
    channel.collect_send_candidates(&mut candidates);

    let summary = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.key,
                candidate.bytes_len,
                candidate.rank.send_order,
                candidate.rank.priority,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        [
            (SendMessageKey::UnreliableSingle(0), 8, 0, 0.0),
            (SendMessageKey::UnreliableFragment(0, 0), 0, 0, 0.0),
        ]
    );
    let MessageRef::Single(single) = channel.message(candidates[0].key) else {
        panic!("a single key names a single");
    };
    assert_eq!(single.bytes, Bytes::from_static(b"pending"));
    let next = channel.next_due(candidates[1].key);
    assert_eq!(next, Some(SendMessageKey::UnreliableFragment(0, 1)));
    let MessageRef::Fragment(fragment) = channel.message(next.unwrap()) else {
        panic!("a fragment key names a fragment");
    };
    assert_eq!(fragment.payload(), [1]);
    assert!(fragment.is_last());
    assert_eq!(channel.next_due(next.unwrap()), None);
}

#[test]
fn reliable_channel_retains_locally_unadmitted_messages() {
    let mut channel = channel(ChannelMode::UnorderedReliable, false);
    channel.buffer(
        Bytes::from_static(b"reliable"),
        1.0,
        CompressionConfig::DISABLED,
    );
    let mut candidates = Vec::new();
    channel.collect_send_candidates(&mut candidates);
    assert_eq!(candidates.len(), 1);
    channel.finish_send(SendFlushOutcome::BudgetLimited);
    candidates.clear();
    channel.collect_send_candidates(&mut candidates);
    assert_eq!(candidates.len(), 1);
}

#[test]
fn unreliable_channel_can_discard_locally_unadmitted_messages() {
    let mut channel = channel(ChannelMode::UnorderedUnreliable, false);
    channel.buffer(
        Bytes::from_static(b"stale"),
        1.0,
        CompressionConfig::DISABLED,
    );
    let mut candidates = Vec::new();
    channel.collect_send_candidates(&mut candidates);
    assert_eq!(candidates.len(), 1);
    channel.finish_send(SendFlushOutcome::BudgetLimited);
    candidates.clear();
    channel.collect_send_candidates(&mut candidates);
    assert!(candidates.is_empty());
}

#[test]
fn discard_policy_only_applies_after_a_budget_limit() {
    let mut channel = channel(ChannelMode::UnorderedUnreliable, false);
    channel.buffer(
        Bytes::from_static(b"stale"),
        1.0,
        CompressionConfig::DISABLED,
    );

    let mut candidates = Vec::new();
    channel.collect_send_candidates(&mut candidates);
    channel.finish_send(SendFlushOutcome::Complete);
    candidates.clear();
    channel.collect_send_candidates(&mut candidates);
    assert_eq!(candidates.len(), 1);

    channel.finish_send(SendFlushOutcome::BudgetLimited);
    candidates.clear();
    channel.collect_send_candidates(&mut candidates);
    assert!(candidates.is_empty());
}

/// A budget limit drops a fragmented message no packet started, and keeps one it started, whose
/// fragments the receiver waits for: of 3 fragments, after fragment 0 goes, the message's
/// candidate names fragment 1, and 2 follows it.
#[test]
fn discard_policy_finishes_started_fragmented_messages() {
    let message_len = FRAGMENT_SIZE * 2 + 1;

    let mut wholly_unsent = channel(ChannelMode::UnorderedUnreliable, false);
    wholly_unsent.buffer(
        Bytes::from(vec![0; message_len]),
        1.0,
        CompressionConfig::DISABLED,
    );
    let mut candidates = Vec::new();
    wholly_unsent.collect_send_candidates(&mut candidates);
    assert_eq!(candidates.len(), 1);
    wholly_unsent.finish_send(SendFlushOutcome::BudgetLimited);
    candidates.clear();
    wholly_unsent.collect_send_candidates(&mut candidates);
    assert!(candidates.is_empty());

    let mut partially_sent = channel(ChannelMode::UnorderedUnreliable, false);
    partially_sent.buffer(
        Bytes::from(vec![0; message_len]),
        1.0,
        CompressionConfig::DISABLED,
    );
    partially_sent.collect_send_candidates(&mut candidates);
    partially_sent.commit_send(candidates[0].key);
    partially_sent.finish_send(SendFlushOutcome::BudgetLimited);
    candidates.clear();
    partially_sent.collect_send_candidates(&mut candidates);
    let keys = candidates
        .iter()
        .map(|candidate| candidate.key)
        .collect::<Vec<_>>();
    assert_eq!(keys, [SendMessageKey::UnreliableFragment(0, 1)]);
    assert_eq!(
        partially_sent.next_due(keys[0]),
        Some(SendMessageKey::UnreliableFragment(0, 2))
    );
}

#[test]
fn a_budget_limit_does_not_discard_before_send_frequency() {
    let mut channel = ChannelSender::new(
        ChannelKind::of::<TestChannel>(),
        NetId::new(0),
        core::any::type_name::<TestChannel>(),
        &ChannelConfig {
            mode: ChannelMode::UnorderedUnreliable,
            send_frequency: Duration::from_secs(1),
            retry_unsent_messages: false,
            ..ChannelConfig::default()
        },
    );
    channel.buffer(
        Bytes::from_static(b"not-yet-eligible"),
        1.0,
        CompressionConfig::DISABLED,
    );

    let mut candidates = Vec::new();
    channel.collect_send_candidates(&mut candidates);
    assert!(candidates.is_empty());
    channel.finish_send(SendFlushOutcome::BudgetLimited);

    channel.update(Duration::from_secs(1));
    channel.collect_send_candidates(&mut candidates);
    assert_eq!(candidates.len(), 1);
}

#[test]
fn reliable_fragment_ack_reports_completion_once_and_clears_retries() {
    let mut channel = channel(ChannelMode::UnorderedReliable, true);
    assert_eq!(
        channel.buffer(
            Bytes::from(vec![0; FRAGMENT_SIZE + 1]),
            1.0,
            CompressionConfig::DISABLED,
        ),
        Some(MessageId(0))
    );

    assert!(!channel.receive_ack(fragment_ack(0)));
    assert!(!channel.receive_ack(fragment_ack(0)));
    assert!(channel.receive_ack(fragment_ack(1)));
    assert!(!channel.receive_ack(fragment_ack(1)));

    let mut candidates = Vec::new();
    channel.collect_send_candidates(&mut candidates);
    assert!(candidates.is_empty());
}

#[test]
fn message_id_policy_is_derived_from_delivery_mode() {
    let mut unordered = channel(ChannelMode::UnorderedUnreliable, true);
    assert_eq!(
        unordered.buffer(
            Bytes::from_static(b"single"),
            1.0,
            CompressionConfig::DISABLED,
        ),
        None
    );
    assert_eq!(
        unordered.buffer(
            Bytes::from(vec![0; FRAGMENT_SIZE + 1]),
            1.0,
            CompressionConfig::DISABLED,
        ),
        Some(MessageId(0))
    );

    let mut sequenced = channel(ChannelMode::SequencedUnreliable, true);
    assert_eq!(
        sequenced.buffer(
            Bytes::from_static(b"single"),
            1.0,
            CompressionConfig::DISABLED,
        ),
        Some(MessageId(0))
    );
}

/// A reliable message waits while its packet is in flight; the packet's loss sends it again,
/// which is its second send and its only resend, and its ack ends it.
#[test]
fn every_reliable_mode_sends_again_at_a_loss_and_stops_after_ack() {
    let ack = SendMessageKey::ReliableSingle(MessageId(0));
    for mode in [
        ChannelMode::UnorderedReliable,
        ChannelMode::SequencedReliable,
        ChannelMode::OrderedReliable,
    ] {
        let mut channel = channel(mode, true);
        channel.buffer(
            Bytes::from_static(b"reliable"),
            1.0,
            CompressionConfig::DISABLED,
        );
        let mut candidates = Vec::new();
        channel.collect_send_candidates(&mut candidates);
        assert_eq!(candidates.len(), 1);
        assert!(!channel.commit_send(candidates[0].key));

        candidates.clear();
        channel.collect_send_candidates(&mut candidates);
        assert!(candidates.is_empty(), "{mode:?}: a message in flight waits");

        channel.receive_loss(ack);
        channel.collect_send_candidates(&mut candidates);
        assert_eq!(candidates.len(), 1);
        assert!(channel.commit_send(candidates[0].key), "{mode:?}");
        assert_eq!(channel.sent_messages.get(), 2, "{mode:?}");
        assert!(channel.receive_ack(ack));
        candidates.clear();
        channel.collect_send_candidates(&mut candidates);
        assert!(candidates.is_empty());
    }
}

/// The setting bounds a message unless its link's fragments are too small to carry it in
/// `MAX_FRAGMENTS`: 16,383 fragments of 1 byte are 16,383 bytes, under the 512 KiB default;
/// of `FRAGMENT_SIZE` bytes they are above it.
#[test]
fn max_message_size_is_the_setting_or_the_fragment_bound() {
    let sender = ChannelSender::new(
        ChannelKind::of::<TestChannel>(),
        NetId::new(0),
        core::any::type_name::<TestChannel>(),
        &ChannelConfig::default(),
    );
    assert_eq!(sender.max_message_size(1), 16_383);
    assert_eq!(
        sender.max_message_size(FRAGMENT_SIZE),
        DEFAULT_MAX_MESSAGE_SIZE
    );
}

/// A sequenced reliable channel's receiver discards older messages, so message 1 drops message
/// 0 from the sender: only message 1 is sent again, and message 0's late ack completes nothing.
#[test]
fn a_newer_sequenced_reliable_message_drops_the_older() {
    let mut channel = channel(ChannelMode::SequencedReliable, true);
    channel.buffer(
        Bytes::from_static(b"zero"),
        1.0,
        CompressionConfig::DISABLED,
    );
    let mut candidates = Vec::new();
    channel.collect_send_candidates(&mut candidates);
    channel.commit_send(candidates[0].key);
    channel.buffer(Bytes::from_static(b"one"), 1.0, CompressionConfig::DISABLED);

    assert_eq!(channel.pending_reliable(), [MessageId(1)]);
    assert!(!channel.receive_ack(SendMessageKey::ReliableSingle(MessageId(0))));
    candidates.clear();
    channel.collect_send_candidates(&mut candidates);
    let keys = candidates
        .iter()
        .map(|candidate| candidate.key)
        .collect::<Vec<_>>();
    assert_eq!(keys, [SendMessageKey::ReliableSingle(MessageId(1))]);
}

/// Priority grows by its base each second a message waits: a retained unreliable message of
/// base 2 reads 0.2 after 100 ms and 0.4 after 200 ms; a frame of zero delta adds nothing. A
/// reliable message restarts when sent and grows only while it is due: its retransmission, due
/// at its packet's loss, competes at 0.051 after the 51 ms frame it was due in, as a fresh
/// message of base 1 would, not at its age.
#[test]
fn priority_accumulates_while_waiting_and_restarts_when_sent() {
    let priorities = |channel: &mut ChannelSender| {
        let mut candidates = Vec::new();
        channel.collect_send_candidates(&mut candidates);
        candidates
            .iter()
            .map(|candidate| candidate.rank.priority)
            .collect::<Vec<_>>()
    };

    let mut unreliable = channel(ChannelMode::UnorderedUnreliable, true);
    unreliable.buffer(Bytes::from_static(b"m"), 2.0, CompressionConfig::DISABLED);
    unreliable.update(Duration::from_millis(100));
    assert_eq!(priorities(&mut unreliable), [0.2]);
    unreliable.update(Duration::from_millis(100));
    assert_eq!(priorities(&mut unreliable), [0.4]);
    unreliable.update(Duration::from_millis(0));
    assert_eq!(priorities(&mut unreliable), [0.4]);

    let mut channel = channel(ChannelMode::OrderedReliable, true);
    channel.buffer(Bytes::from_static(b"m"), 1.0, CompressionConfig::DISABLED);
    channel.update(Duration::from_millis(100));
    let mut candidates = Vec::new();
    channel.collect_send_candidates(&mut candidates);
    assert_eq!(candidates[0].rank.priority, 0.1);
    channel.commit_send(candidates[0].key);
    // Waiting for its packet's fate is not waiting to be sent: priority stays at zero until the
    // loss makes it due.
    channel.update(Duration::from_millis(50));
    assert_eq!(priorities(&mut channel), Vec::<f32>::new());
    channel.receive_loss(SendMessageKey::ReliableSingle(MessageId(0)));
    channel.update(Duration::from_millis(51));
    assert_eq!(priorities(&mut channel), [0.051]);
}
