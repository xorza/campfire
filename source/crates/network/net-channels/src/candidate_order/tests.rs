use core::num::NonZeroU32;

use campfire_net_core::NetId;

use super::*;
use crate::message_id::MessageId;
use crate::send_candidate::CandidateRank;
use crate::send_message_key::SendMessageKey;

/// A candidate of `channel_id` of a whole message of `bytes_len` bytes, or of a fragmented
/// message when `bytes_len` is 0.
fn candidate(
    channel_id: u16,
    key: SendMessageKey,
    priority: f32,
    send_order: u32,
    bytes_len: u32,
) -> SendCandidate {
    SendCandidate {
        channel_id: NetId::new(channel_id),
        key,
        rank: CandidateRank {
            priority,
            send_order,
        },
        single_len: NonZeroU32::new(bytes_len),
    }
}

fn order(candidates: &CandidateOrder) -> Vec<(NetId, SendMessageKey)> {
    candidates
        .candidates()
        .iter()
        .map(|candidate| (candidate.channel_id, candidate.key))
        .collect()
}

/// An infinite score goes before 120 candidates that score 1.
#[test]
fn an_infinite_priority_goes_before_a_burst_of_traffic() {
    let mut candidates = CandidateOrder::default();
    candidates
        .candidates_mut()
        .extend((0..120).map(|index| candidate(1, SendMessageKey::new(index, 0), 1.0, index, 9)));
    candidates
        .candidates_mut()
        .push(candidate(0, SendMessageKey::new(0, 0), f32::INFINITY, 0, 5));

    candidates.prioritize();

    assert_eq!(
        order(&candidates)[0],
        (NetId::new(0), SendMessageKey::new(0, 0))
    );
}

/// A single that scores 10 goes before a fragmented message that scores 1, although fragmented
/// messages go first within one score.
#[test]
fn a_single_of_higher_priority_goes_before_a_fragment_of_lower() {
    let mut candidates = CandidateOrder::default();
    candidates.candidates_mut().extend([
        candidate(0, SendMessageKey::new(100, 0), 1.0, 0, 0),
        candidate(1, SendMessageKey::new(0, 0), 10.0, 0, 7),
    ]);

    candidates.prioritize();

    assert_eq!(
        order(&candidates),
        [
            (NetId::new(1), SendMessageKey::new(0, 0)),
            (NetId::new(0), SendMessageKey::new(100, 0)),
        ]
    );
}

#[test]
fn at_one_priority_fragments_go_first_then_singles_by_size() {
    let mut candidates = CandidateOrder::default();
    candidates.candidates_mut().extend([
        candidate(0, SendMessageKey::new(0, 0), 1.0, 0, 14),
        candidate(0, SendMessageKey::new(1, 0), 1.0, 1, 6),
        candidate(0, SendMessageKey::new(100, 0), 1.0, 0, 0),
    ]);

    candidates.prioritize();

    assert_eq!(
        order(&candidates),
        [
            (NetId::new(0), SendMessageKey::new(100, 0)),
            (NetId::new(0), SendMessageKey::new(1, 0)),
            (NetId::new(0), SendMessageKey::new(0, 0)),
        ]
    );
}

/// Equal sizes fall back to the send order: two singles of 6 bytes go oldest first.
#[test]
fn singles_of_one_size_go_in_their_send_order() {
    let mut candidates = CandidateOrder::default();
    candidates.candidates_mut().extend([
        candidate(0, SendMessageKey::new(1, 0), 1.0, 1, 6),
        candidate(0, SendMessageKey::new(0, 0), 1.0, 0, 6),
    ]);

    candidates.prioritize();

    assert_eq!(
        order(&candidates),
        [
            (NetId::new(0), SendMessageKey::new(0, 0)),
            (NetId::new(0), SendMessageKey::new(1, 0)),
        ]
    );
}

/// Fragmented messages of one channel go in their queue order, and those of two channels at
/// one send order by channel id.
#[test]
fn fragments_go_in_their_queue_order_then_by_channel() {
    let mut candidates = CandidateOrder::default();
    candidates.candidates_mut().extend(
        [3, 1, 2, 0].map(|slot| candidate(0, SendMessageKey::new(100 + slot, 0), 1.0, slot, 0)),
    );
    candidates
        .candidates_mut()
        .push(candidate(1, SendMessageKey::new(100, 0), 1.0, 0, 0));

    candidates.prioritize();

    assert_eq!(
        order(&candidates),
        [
            (NetId::new(0), SendMessageKey::new(100, 0)),
            (NetId::new(1), SendMessageKey::new(100, 0)),
            (NetId::new(0), SendMessageKey::new(100 + 1, 0)),
            (NetId::new(0), SendMessageKey::new(100 + 2, 0)),
            (NetId::new(0), SendMessageKey::new(100 + 3, 0)),
        ]
    );
}

/// A reliable channel's send order is a message's place in its window, so message u16::MAX,
/// the window's first, goes before message 0 past the id's wrap.
#[test]
fn a_reliable_fragment_order_holds_across_the_wrap_of_the_message_id() {
    let mut candidates = CandidateOrder::default();
    for (message_id, send_order) in [(MessageId(0), 1), (MessageId(u16::MAX), 0)] {
        candidates.candidates_mut().push(candidate(
            0,
            SendMessageKey::of_id(message_id, 0),
            1.0,
            send_order,
            0,
        ));
    }

    candidates.prioritize();

    assert_eq!(
        order(&candidates),
        [MessageId(u16::MAX), MessageId(0)].map(|id| (NetId::new(0), SendMessageKey::of_id(id, 0)))
    );
}
