use bytes::Bytes;
use core::time::Duration;
use std::collections::VecDeque;

use crate::compression_config::CompressionConfig;
use crate::compression_scratch::CompressionScratch;
use crate::fragmented_message::{FragmentState, FragmentedMessage};
use crate::message_id::MessageId;
use crate::message_ref::MessageRef;
use crate::send_candidate::{CandidateRank, CandidateSink};
use crate::send_message_key::SendMessageKey;
use crate::single_data::SingleData;

/// A reliable message: one packet's or a fragmented one's, and where its parts are. A part goes
/// in the next packet that has room unless a packet that carries it is in flight
/// (13-net-stack/session.md, Loss is found by the packet threshold of 3, or by 9/8 of the RTT).
#[derive(Debug)]
enum ReliableMessage {
    Single {
        data: SingleData,
        state: FragmentState,
    },
    Fragmented(FragmentedMessage),
}

impl ReliableMessage {
    const fn set_id(&mut self, id: MessageId) {
        match self {
            Self::Single { data, .. } => data.id = Some(id),
            Self::Fragmented(message) => message.set_message_id(id),
        }
    }

    /// The key of its first due part, if one is due.
    fn first_due(&mut self, id: MessageId) -> Option<SendMessageKey> {
        match self {
            Self::Single { state, .. } => (*state == FragmentState::Unsent
                || *state == FragmentState::Lost)
                .then_some(SendMessageKey::ReliableSingle(id)),
            Self::Fragmented(message) => message
                .first_due()
                .map(|index| SendMessageKey::ReliableFragment(id, index)),
        }
    }
}

/// The most message ids a reliable channel has in flight, from its oldest unacked message to its
/// next id.
///
/// A receiver takes an id up to 32,767 past its own oldest undelivered one as new, and lags the
/// sender by the messages it received but has not delivered yet; half that range leaves the other
/// half for the lag, and keeps every id in flight distinct in the 65,536-id space.
pub(crate) const SEND_WINDOW: u16 = 16_384;

/// A message waiting for room in the send window, already split and compressed: only its id is
/// missing.
#[derive(Debug)]
struct WaitingMessage {
    message: ReliableMessage,
    priority: f32,
}

/// Which messages of a reliable channel the receiver needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReliableDelivery {
    /// Every message, in order or not.
    Every,
    /// Only the newest message: a sequenced channel's receiver discards the older ones, so a
    /// newer message drops them from the sender too.
    Newest,
}

#[derive(Debug)]
struct PendingReliableMessage {
    message: ReliableMessage,
    base_priority: f32,
    /// Grows by `base_priority` a second while the message waits to be sent, and restarts when it
    /// is sent, so a retransmission competes as a fresh message does.
    accumulated_priority: f32,
    /// Whether the message is in the due list.
    listed: bool,
}

/// State which differs specifically for reliable channel sends.
#[derive(Debug)]
pub(super) struct ReliableSendState {
    delivery: ReliableDelivery,
    /// The messages of the window, at their id's distance from `oldest_unacked`; an acked one
    /// leaves its slot empty until the slots before it are.
    pending: VecDeque<Option<PendingReliableMessage>>,
    /// The ids of the pending messages that have a part to send, which a collection reads instead
    /// of every pending message; one that sent all since leaves at the next collection.
    due: Vec<MessageId>,
    /// Messages buffered while the window is full, in buffering order.
    waiting: VecDeque<WaitingMessage>,
    /// The id of `pending`'s first slot.
    oldest_unacked: MessageId,
}

impl ReliableSendState {
    pub(super) const fn new(delivery: ReliableDelivery) -> Self {
        Self {
            delivery,
            pending: VecDeque::new(),
            due: Vec::new(),
            waiting: VecDeque::new(),
            oldest_unacked: MessageId(0),
        }
    }

    /// The id the next admitted message takes.
    fn next_message_id(&self) -> MessageId {
        self.id_at(self.pending.len())
    }

    /// The id of the window's slot `slot`.
    fn id_at(&self, slot: usize) -> MessageId {
        MessageId(
            self.oldest_unacked
                .0
                .wrapping_add(u16::try_from(slot).unwrap()),
        )
    }

    /// The slot of `id`, if it is in the window.
    fn slot(&self, id: MessageId) -> Option<usize> {
        let index = usize::from(id.0.wrapping_sub(self.oldest_unacked.0));
        (index < self.pending.len()).then_some(index)
    }

    fn pending_mut(&mut self, id: MessageId) -> Option<&mut PendingReliableMessage> {
        let slot = self.slot(id)?;
        self.pending[slot].as_mut()
    }

    /// Buffers a message, and returns its id when the send window has room for it now; it
    /// waits for room otherwise, in buffering order.
    pub(super) fn buffer_send(
        &mut self,
        message: Bytes,
        priority: f32,
        fragment_size: usize,
        compression: CompressionConfig,
        compression_scratch: &mut CompressionScratch,
    ) -> Option<MessageId> {
        // The id is set when the message enters the window.
        let message = if message.len() > fragment_size {
            ReliableMessage::Fragmented(FragmentedMessage::split(
                MessageId(0),
                message,
                fragment_size,
                compression,
                compression_scratch,
            ))
        } else {
            ReliableMessage::Single {
                data: SingleData::new(None, message),
                state: FragmentState::Unsent,
            }
        };
        if self.delivery == ReliableDelivery::Newest {
            self.waiting.clear();
        }
        self.waiting.push_back(WaitingMessage { message, priority });
        let admitted_before = self.next_message_id();
        self.admit_waiting();
        (self.waiting.is_empty() && self.next_message_id() != admitted_before)
            .then(|| self.next_message_id() - 1)
    }

    /// Moves waiting messages into the window while it has room.
    fn admit_waiting(&mut self) {
        while self.pending.len() < usize::from(SEND_WINDOW)
            && let Some(WaitingMessage {
                mut message,
                priority,
            }) = self.waiting.pop_front()
        {
            if self.delivery == ReliableDelivery::Newest {
                self.oldest_unacked = self.next_message_id();
                self.pending.clear();
                self.due.clear();
            }
            let message_id = self.next_message_id();
            message.set_id(message_id);
            self.pending.push_back(Some(PendingReliableMessage {
                message,
                base_priority: priority,
                accumulated_priority: 0.0,
                listed: true,
            }));
            self.due.push(message_id);
        }
    }

    /// Appends the messages due for sending, after growing their priority by `elapsed` since the
    /// last collection.
    pub(super) fn collect_candidates(&mut self, elapsed: Duration, sink: &mut CandidateSink<'_>) {
        self.admit_waiting();
        let elapsed_secs = elapsed.as_secs_f32();
        let mut due = core::mem::take(&mut self.due);
        due.retain(|&message_id| {
            let Some(slot) = self.slot(message_id) else {
                return false;
            };
            let Some(pending) = &mut self.pending[slot] else {
                return false;
            };
            let Some(key) = pending.message.first_due(message_id) else {
                pending.listed = false;
                return false;
            };
            pending.accumulated_priority += pending.base_priority * elapsed_secs;
            let rank = CandidateRank {
                priority: pending.accumulated_priority,
                send_order: u32::try_from(slot).unwrap(),
            };
            let message = match &pending.message {
                ReliableMessage::Single { data, .. } => MessageRef::Single(data),
                ReliableMessage::Fragmented(message) => {
                    let SendMessageKey::ReliableFragment(_, index) = key else {
                        unreachable!("a fragmented message's key names a fragment")
                    };
                    MessageRef::Fragment(message.fragment(index))
                }
            };
            sink.push(key, rank, message);
            true
        });
        self.due = due;
    }

    pub(super) fn message(&self, key: SendMessageKey) -> MessageRef<'_> {
        let (SendMessageKey::ReliableSingle(id) | SendMessageKey::ReliableFragment(id, _)) = key
        else {
            unreachable!("a reliable channel names its messages by message id")
        };
        let pending = self
            .slot(id)
            .and_then(|slot| self.pending[slot].as_ref())
            .expect("a send key names a pending message");
        match (&pending.message, key) {
            (ReliableMessage::Single { data, .. }, SendMessageKey::ReliableSingle(_)) => {
                MessageRef::Single(data)
            }
            (ReliableMessage::Fragmented(message), SendMessageKey::ReliableFragment(_, index)) => {
                MessageRef::Fragment(message.fragment(index))
            }
            _ => unreachable!("a send key matches the shape of the message it names"),
        }
    }

    /// The key of the next due fragment of the message whose fragment `key` names, past it.
    pub(super) fn next_due(&self, key: SendMessageKey) -> Option<SendMessageKey> {
        let SendMessageKey::ReliableFragment(id, index) = key else {
            unreachable!("only a fragment has a next fragment")
        };
        let pending = self.slot(id).and_then(|slot| self.pending[slot].as_ref())?;
        let ReliableMessage::Fragmented(message) = &pending.message else {
            unreachable!("a fragment key names a fragmented message")
        };
        message
            .next_due(index + 1)
            .map(|next| key.with_fragment(next))
    }

    /// Records that a packet carries the message or fragment `key` names, and returns whether a
    /// packet carried it before.
    pub(super) fn commit_send(&mut self, key: SendMessageKey) -> bool {
        let (SendMessageKey::ReliableSingle(id) | SendMessageKey::ReliableFragment(id, _)) = key
        else {
            unreachable!("a reliable channel names its messages by message id")
        };
        let pending = self
            .pending_mut(id)
            .expect("a committed key names a pending message");
        pending.accumulated_priority = 0.0;
        match (&mut pending.message, key) {
            (ReliableMessage::Single { state, .. }, SendMessageKey::ReliableSingle(_)) => {
                core::mem::replace(state, FragmentState::InFlight) == FragmentState::Lost
            }
            (ReliableMessage::Fragmented(message), SendMessageKey::ReliableFragment(_, index)) => {
                message.commit(index)
            }
            _ => unreachable!("a send key matches the shape of the message it names"),
        }
    }

    /// Applies the loss of a packet that carried the message or fragment `key` names: it goes in
    /// the next packet that has room. A message acknowledged since, or dropped by a newer one,
    /// needs nothing.
    pub(super) fn receive_loss(&mut self, key: SendMessageKey) {
        let (SendMessageKey::ReliableSingle(id) | SendMessageKey::ReliableFragment(id, _)) = key
        else {
            unreachable!("only a reliable message's packet is watched")
        };
        let Some(pending) = self.pending_mut(id) else {
            return;
        };
        match (&mut pending.message, key) {
            (ReliableMessage::Single { state, .. }, SendMessageKey::ReliableSingle(_)) => {
                if *state == FragmentState::InFlight {
                    *state = FragmentState::Lost;
                }
            }
            (ReliableMessage::Fragmented(message), SendMessageKey::ReliableFragment(_, index)) => {
                message.lose(index);
            }
            _ => unreachable!("a loss matches the shape of the message it names"),
        }
        if !pending.listed {
            pending.listed = true;
            self.due.push(id);
        }
    }

    /// Applies an acknowledgement and returns whether it completed the logical message.
    ///
    /// The window keeps every id in flight distinct, so an ack matches the shape of the message
    /// it names: one recorded for the packet that carried it.
    pub(super) fn receive_ack(&mut self, key: SendMessageKey) -> bool {
        let (SendMessageKey::ReliableSingle(id) | SendMessageKey::ReliableFragment(id, _)) = key
        else {
            unreachable!("only a reliable message's packet is watched")
        };
        let Some(slot) = self.slot(id) else {
            return false;
        };
        let Some(pending) = &mut self.pending[slot] else {
            return false;
        };
        let completed = match (&mut pending.message, key) {
            (ReliableMessage::Single { .. }, SendMessageKey::ReliableSingle(_)) => true,
            (ReliableMessage::Fragmented(message), SendMessageKey::ReliableFragment(_, index)) => {
                message.ack(index)
            }
            _ => unreachable!("an ack matches the shape of the message it names"),
        };
        if completed {
            self.pending[slot] = None;
            while self.pending.front().is_some_and(Option::is_none) {
                self.pending.pop_front();
                self.oldest_unacked += 1;
            }
        }
        completed
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::message_id::MessageId;
    use crate::reliable_send_state::ReliableSendState;

    impl ReliableSendState {
        pub(crate) fn pending_ids(&self) -> impl Iterator<Item = MessageId> + '_ {
            self.pending
                .iter()
                .enumerate()
                .filter(|(_, pending)| pending.is_some())
                .map(|(slot, _)| self.id_at(slot))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::FRAGMENT_SIZE;
    use crate::send_candidate::SendCandidate;
    use campfire_net_core::NetId;

    fn send(state: &mut ReliableSendState, bytes: Bytes) -> Option<MessageId> {
        state.buffer_send(
            bytes,
            1.0,
            FRAGMENT_SIZE,
            CompressionConfig::DISABLED,
            &mut CompressionScratch::default(),
        )
    }

    fn collect(state: &mut ReliableSendState) -> Vec<SendCandidate> {
        let mut candidates = Vec::new();
        state.collect_candidates(
            Duration::ZERO,
            &mut CandidateSink {
                channel_id: NetId::new(0),
                channel_priority: 1.0,
                candidates: &mut candidates,
            },
        );
        candidates
    }

    /// Ids wrap at 65,536 and the window's order does not: messages u16::MAX and 0 send in that
    /// order, at their distance from the oldest unacked one, and the ack of the first leaves the
    /// second at the window's start.
    #[test]
    fn pending_send_order_and_lookup_survive_message_id_rollover() {
        let mut state = ReliableSendState::new(ReliableDelivery::Every);
        state.oldest_unacked = MessageId(u16::MAX);
        assert_eq!(
            send(&mut state, Bytes::from_static(b"max")),
            Some(MessageId(u16::MAX))
        );
        assert_eq!(
            send(&mut state, Bytes::from_static(b"zero")),
            Some(MessageId(0))
        );

        let order = collect(&mut state)
            .iter()
            .map(|candidate| (candidate.key, candidate.rank.send_order))
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            [
                (SendMessageKey::ReliableSingle(MessageId(u16::MAX)), 0),
                (SendMessageKey::ReliableSingle(MessageId(0)), 1)
            ]
        );

        assert!(state.receive_ack(SendMessageKey::ReliableSingle(MessageId(u16::MAX))));
        assert_eq!(state.oldest_unacked, MessageId(0));
        let order = collect(&mut state)
            .iter()
            .map(|candidate| (candidate.key, candidate.rank.send_order))
            .collect::<Vec<_>>();
        assert_eq!(order, [(SendMessageKey::ReliableSingle(MessageId(0)), 0)]);
    }

    /// The window holds 16,384 ids from the oldest unacked message: the 16,385th and 16,386th
    /// messages wait. An ack of message 5 frees no room, as message 0 still holds the window's
    /// start; the ack of message 0 admits the 16,385th as id 16,384, and moves the start to 1.
    #[test]
    fn the_send_window_holds_messages_until_the_oldest_is_acked() {
        let mut state = ReliableSendState::new(ReliableDelivery::Every);
        for id in 0..SEND_WINDOW {
            assert_eq!(
                send(&mut state, Bytes::from_static(b"m")),
                Some(MessageId(id))
            );
        }
        assert_eq!(send(&mut state, Bytes::from_static(b"m")), None);
        assert_eq!(send(&mut state, Bytes::from_static(b"m")), None);
        assert_eq!(state.pending.len(), usize::from(SEND_WINDOW));

        let ack = |id| SendMessageKey::ReliableSingle(MessageId(id));
        assert!(state.receive_ack(ack(5)));
        collect(&mut state);
        assert_eq!(state.slot(MessageId(SEND_WINDOW)), None);

        assert!(state.receive_ack(ack(0)));
        assert_eq!(state.oldest_unacked, MessageId(1));
        collect(&mut state);
        assert!(state.pending_mut(MessageId(SEND_WINDOW)).is_some());
        assert_eq!(state.waiting.len(), 1);
    }

    /// A sent message waits while its packet is in flight, off the due list; the packet's loss
    /// lists it again and its next commit is a resend. Its ack ends it, and a loss after the ack
    /// changes nothing.
    #[test]
    fn a_message_goes_again_when_its_packet_is_lost() {
        let mut state = ReliableSendState::new(ReliableDelivery::Every);
        assert_eq!(
            send(&mut state, Bytes::from_static(b"m")),
            Some(MessageId(0))
        );
        let candidates = collect(&mut state);
        assert_eq!(candidates.len(), 1);
        let key = candidates[0].key;
        assert!(!state.commit_send(key), "the first send is no resend");

        assert!(collect(&mut state).is_empty(), "a message in flight waits");
        assert!(state.due.is_empty());

        state.receive_loss(key);
        assert_eq!(state.due, [MessageId(0)]);
        assert_eq!(collect(&mut state).len(), 1);
        assert!(state.commit_send(key), "the send after a loss is a resend");

        assert!(state.receive_ack(key));
        state.receive_loss(key);
        assert!(collect(&mut state).is_empty());
    }

    /// A fragmented message is one candidate, which names its first due fragment, and the next
    /// due fragment follows each: of 3 fragments, after 0 and 2 go and 2 is lost, the candidate
    /// names 1, and 2 follows it.
    #[test]
    fn a_fragmented_message_is_one_candidate_with_its_first_due_fragment() {
        let mut state = ReliableSendState::new(ReliableDelivery::Every);
        send(&mut state, Bytes::from(vec![0; FRAGMENT_SIZE * 5 / 2]));
        let fragment = |index| SendMessageKey::ReliableFragment(MessageId(0), index);
        let candidates = collect(&mut state);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key, fragment(0));
        assert_eq!(state.next_due(fragment(0)), Some(fragment(1)));
        assert_eq!(state.next_due(fragment(2)), None);

        state.commit_send(fragment(0));
        state.commit_send(fragment(2));
        state.receive_loss(fragment(2));
        let candidates = collect(&mut state);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key, fragment(1));
        assert_eq!(state.next_due(fragment(1)), Some(fragment(2)));
    }
}
