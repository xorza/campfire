use core::time::Duration;
use std::collections::VecDeque;

use bytes::Bytes;

use crate::fragmented_message::FragmentedMessage;
use crate::message_id::MessageId;
use crate::message_ref::MessageRef;
use crate::queued_message::QueuedMessage;
use crate::send_candidate::{CandidateRank, CandidateSink};
use crate::send_message_key::SendMessageKey;
use crate::single_body::SingleBody;

/// The most message ids a reliable channel has in flight, from its oldest unacked message to its
/// next id.
///
/// A receiver takes an id up to 32,767 past its own oldest undelivered one as new, and lags the
/// sender by the messages it received but has not delivered yet; half that range leaves the other
/// half for the lag, and keeps every id in flight distinct in the 65,536-id space.
pub(crate) const SEND_WINDOW: u16 = 16_384;

/// A message waiting for room in the send window, already split: only its id is
/// missing.
#[derive(Debug)]
struct WaitingMessage {
    message: QueuedMessage,
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
struct PendingQueuedMessage {
    message: QueuedMessage,
    base_priority: f32,
    /// Grows by `base_priority` a second while the message waits to be sent, and restarts when it
    /// is sent, so a retransmission competes as a fresh message does.
    accumulated_priority: f32,
    /// Whether the message is in the due list.
    listed: bool,
}

/// State which differs specifically for reliable channel sends (11-net-stack/transport.md, A
/// reliable message goes again when the packet session finds its packet lost).
#[derive(Debug)]
pub(super) struct ReliableSendState {
    delivery: ReliableDelivery,
    /// The messages of the window, at their id's distance from `oldest_unacked`; an acked one
    /// leaves its slot empty until the slots before it are.
    pending: VecDeque<Option<PendingQueuedMessage>>,
    /// The ids of the pending messages that have a part to send, which a collection reads instead
    /// of every pending message; one that sent all since leaves at the next collection.
    due: Vec<MessageId>,
    /// Messages buffered while the window is full, in buffering order.
    waiting: VecDeque<WaitingMessage>,
    /// The id of `pending`'s first slot.
    oldest_unacked: MessageId,
    /// The bytes of the messages of `pending` and `waiting`.
    queued: usize,
}

impl ReliableSendState {
    pub(super) const fn new(delivery: ReliableDelivery) -> Self {
        Self {
            delivery,
            pending: VecDeque::new(),
            due: Vec::new(),
            waiting: VecDeque::new(),
            oldest_unacked: MessageId(0),
            queued: 0,
        }
    }

    /// The bytes of the messages the channel holds until they are acknowledged.
    pub(super) const fn queued(&self) -> usize {
        self.queued
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

    fn pending_mut(&mut self, id: MessageId) -> Option<&mut PendingQueuedMessage> {
        let slot = self.slot(id)?;
        self.pending[slot].as_mut()
    }

    /// Buffers a message, and returns its id when the send window has room for it now; it
    /// waits for room otherwise, in buffering order.
    pub(super) fn buffer_send(
        &mut self,
        message: Bytes,
        priority: f32,
        fragment_len: usize,
    ) -> Option<MessageId> {
        // The id is set when the message enters the window.
        let message = if message.len() > fragment_len {
            QueuedMessage::Fragmented(FragmentedMessage::split(
                MessageId(0),
                message,
                fragment_len,
            ))
        } else {
            QueuedMessage::whole(SingleBody::new(None, message))
        };
        if self.delivery == ReliableDelivery::Newest {
            for waiting in self.waiting.drain(..) {
                self.queued -= waiting.message.len();
            }
        }
        self.queued += message.len();
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
                for pending in self.pending.drain(..).flatten() {
                    self.queued -= pending.message.len();
                }
                self.due.clear();
            }
            let message_id = self.next_message_id();
            message.set_id(message_id);
            self.pending.push_back(Some(PendingQueuedMessage {
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
            let Some(part) = pending.message.first_due() else {
                pending.listed = false;
                return false;
            };
            pending.accumulated_priority += pending.base_priority * elapsed_secs;
            let rank = CandidateRank {
                priority: pending.accumulated_priority,
                send_order: u32::try_from(slot).unwrap(),
            };
            let key = SendMessageKey::of_id(message_id, part);
            let message = pending.message.part(part);
            sink.push(key, rank, message);
            true
        });
        self.due = due;
    }

    pub(super) fn message(&self, key: SendMessageKey) -> MessageRef<'_> {
        self.slot(key.id())
            .and_then(|slot| self.pending[slot].as_ref())
            .expect("a send key names a pending message")
            .message
            .part(key.part)
    }

    /// The key of the next due part of the message `key` names, past its part.
    pub(super) fn next_due(&self, key: SendMessageKey) -> Option<SendMessageKey> {
        let pending = self
            .slot(key.id())
            .and_then(|slot| self.pending[slot].as_ref())?;
        pending
            .message
            .next_due(key.part + 1)
            .map(|next| key.with_part(next))
    }

    /// Records that a packet carries the part `key` names, and returns whether a packet carried
    /// it before.
    pub(super) fn commit_send(&mut self, key: SendMessageKey) -> bool {
        let pending = self
            .pending_mut(key.id())
            .expect("a committed key names a pending message");
        pending.accumulated_priority = 0.0;
        pending.message.commit(key.part)
    }

    /// Applies the loss of a packet that carried the part `key` names: it goes in the next packet
    /// that has room. A message acknowledged since, or dropped by a newer one, needs nothing.
    pub(super) fn receive_loss(&mut self, key: SendMessageKey) {
        let id = key.id();
        let Some(pending) = self.pending_mut(id) else {
            return;
        };
        pending.message.lose(key.part);
        if !pending.listed {
            pending.listed = true;
            self.due.push(id);
        }
    }

    /// Applies the acknowledgement of the part `key` names, and returns whether it completed the
    /// logical message. The window keeps every id in flight distinct, so an ack names a part of
    /// the message the packet carried.
    pub(super) fn receive_ack(&mut self, key: SendMessageKey) -> bool {
        let Some(slot) = self.slot(key.id()) else {
            return false;
        };
        let Some(pending) = &mut self.pending[slot] else {
            return false;
        };
        let completed = pending.message.ack(key.part);
        if completed {
            self.queued -= pending.message.len();
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
    use crate::packet::FRAGMENT_LEN;
    use crate::send_candidate::SendCandidate;
    use campfire_net_core::NetId;

    fn send(state: &mut ReliableSendState, bytes: Bytes) -> Option<MessageId> {
        state.buffer_send(bytes, 1.0, FRAGMENT_LEN)
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
    fn the_pending_order_and_its_lookup_hold_across_the_wrap_of_the_message_id() {
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
                (SendMessageKey::of_id(MessageId(u16::MAX), 0), 0),
                (SendMessageKey::of_id(MessageId(0), 0), 1)
            ]
        );

        assert!(state.receive_ack(SendMessageKey::of_id(MessageId(u16::MAX), 0)));
        assert_eq!(state.oldest_unacked, MessageId(0));
        let order = collect(&mut state)
            .iter()
            .map(|candidate| (candidate.key, candidate.rank.send_order))
            .collect::<Vec<_>>();
        assert_eq!(order, [(SendMessageKey::of_id(MessageId(0), 0), 0)]);
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

        let ack = |id| SendMessageKey::of_id(MessageId(id), 0);
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
        send(&mut state, Bytes::from(vec![0; FRAGMENT_LEN * 5 / 2]));
        let fragment = |index| SendMessageKey::of_id(MessageId(0), index);
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
