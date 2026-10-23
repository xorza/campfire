//! Channel send state and delivery policies.

use bytes::Bytes;
use core::time::Duration;
use std::collections::VecDeque;

use crate::bench_counter::BenchCounter;
use crate::channel_mode::ChannelMode;
use crate::channel_setup::ChannelSetup;
use crate::channel_slots::ChannelSlots;
use crate::channel_type_id::ChannelTypeId;
use crate::fragmented_message::FragmentedMessage;
use crate::message_id::MessageId;
use crate::message_ref::MessageRef;
use crate::queued_message::QueuedMessage;
use crate::send_candidate::{CandidateMessages, CandidateRank, CandidateSink, SendCandidate};
use crate::send_message_key::SendMessageKey;
use crate::single_body::SingleBody;
use campfire_net_core::NetId;

use crate::reliable_send_state::{ReliableDelivery, ReliableSendState};

/// The sending half of one registered transport channel.
///
/// `ChannelSender` holds messages buffered by the application until the transport either sends or
/// discards them according to the channel's delivery policy. It also applies the channel's send
/// cadence, fragments oversized messages, and sends a reliable message again when its packet is
/// lost.
///
/// Sending is transactional: eligible messages are first exposed as lightweight candidates for
/// global prioritization and packet packing. A candidate is committed only after its packet enters
/// the link's send queue, and the channel finalizes its queues once the complete send flush ends.
/// The private state enum contains only the queueing and acknowledgement state which differs
/// between reliable and unreliable channels.
#[derive(Debug)]
pub(crate) struct ChannelSender {
    channel_type_id: ChannelTypeId,
    channel_id: NetId,
    name: &'static str,
    setup: ChannelSetup,
    timer: Option<SendTimer>,
    /// The time since candidates were last collected, which grows the waiting messages' priority.
    since_collect: Duration,
    state: SendState,
    /// Bytes of this channel's messages in packets given to the packet session.
    pub(crate) sent_bytes: BenchCounter,
    /// This channel's messages and fragments in packets given to the packet session.
    pub(crate) sent_messages: BenchCounter,
}

/// How a send flush ended, which decides whether unsent unreliable messages are dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SendFlushOutcome {
    /// Every candidate that fit was sent, or staging stopped on an error.
    Complete,
    /// The packet session's budget stopped the flush with candidates left.
    BudgetLimited,
}

#[derive(Debug)]
enum SendState {
    Unreliable(UnreliableSendState),
    Reliable(ReliableSendState),
}

#[derive(Debug)]
struct UnreliableSendState {
    delivery: UnreliableDelivery,
    /// The messages in buffering order; a key names one by its index here, for one flush.
    queue: VecDeque<Pending>,
    /// The bytes of the messages of `queue`.
    queued: usize,
    next_message_id: MessageId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnreliableDelivery {
    Unordered,
    Sequenced,
}

/// An unreliable message while it waits to be sent.
#[derive(Debug)]
struct Pending {
    message: QueuedMessage,
    base_priority: f32,
    /// Grows by `base_priority` a second while the message waits to be sent.
    priority: f32,
}

impl Pending {
    const fn new(message: QueuedMessage, base_priority: f32) -> Self {
        Self {
            message,
            base_priority,
            priority: 0.0,
        }
    }
}

impl ChannelSender {
    pub(crate) fn new(
        channel_type_id: ChannelTypeId,
        channel_id: NetId,
        name: &'static str,
        setup: &ChannelSetup,
    ) -> Self {
        let state = match setup.mode {
            ChannelMode::UnorderedUnreliable => {
                SendState::Unreliable(UnreliableSendState::new(UnreliableDelivery::Unordered))
            }
            ChannelMode::SequencedUnreliable => {
                SendState::Unreliable(UnreliableSendState::new(UnreliableDelivery::Sequenced))
            }
            ChannelMode::UnorderedReliable | ChannelMode::OrderedReliable => {
                SendState::Reliable(ReliableSendState::new(ReliableDelivery::Every))
            }
            ChannelMode::SequencedReliable => {
                SendState::Reliable(ReliableSendState::new(ReliableDelivery::Newest))
            }
        };
        let timer = (setup.send_frequency != Duration::default())
            .then(|| SendTimer::new(setup.send_frequency));
        Self {
            channel_type_id,
            channel_id,
            name,
            setup: *setup,
            timer,
            since_collect: Duration::ZERO,
            state,
            sent_bytes: BenchCounter::default(),
            sent_messages: BenchCounter::default(),
        }
    }

    /// Drops every message and counter, as a new sender with the same setup starts.
    pub(crate) fn reset(&mut self) {
        *self = Self::new(
            self.channel_type_id,
            self.channel_id,
            self.name,
            &self.setup,
        );
    }

    /// The channel's type name, which diagnostics show.
    pub(crate) const fn name(&self) -> &'static str {
        self.name
    }

    /// The largest message this channel sends (11-net-stack/transport.md, Each channel has a
    /// maximum message size).
    pub(crate) const fn max_message_len(&self) -> usize {
        self.setup.max_message_len
    }

    /// The bytes of the messages the channel holds until it sends them, or on a reliable channel
    /// until they are acknowledged.
    pub(crate) const fn queued(&self) -> usize {
        match &self.state {
            SendState::Unreliable(state) => state.queued,
            SendState::Reliable(state) => state.queued(),
        }
    }

    /// Whether the transport tracks which packet carried each message, which only reliable
    /// channels need.
    pub(crate) const fn watches_acks(&self) -> bool {
        matches!(&self.state, SendState::Reliable(_))
    }

    /// Advances the channel by a frame of `delta`.
    pub(crate) fn update(&mut self, delta: Duration) {
        if let Some(timer) = &mut self.timer {
            timer.tick(delta);
        }
        self.since_collect += delta;
    }

    /// Buffers `message`, which a link of `fragment_len` splits when it is longer.
    pub(crate) fn buffer_send(
        &mut self,
        message: Bytes,
        priority: f32,
        fragment_len: usize,
    ) -> Option<MessageId> {
        match &mut self.state {
            SendState::Unreliable(state) => state.buffer_send(message, priority, fragment_len),
            SendState::Reliable(state) => state.buffer_send(message, priority, fragment_len),
        }
    }

    /// Appends the messages which are eligible for this send flush.
    ///
    /// Candidates participate in global priority ordering and packet packing, but collecting them
    /// does not remove messages from the channel or mark reliable messages as sent, so an
    /// unsuccessful staging attempt leaves the channel unchanged.
    pub(crate) fn collect_send_candidates(&mut self, output: &mut Vec<SendCandidate>) {
        if !is_ready_to_send(self.timer.as_ref()) {
            return;
        }
        let elapsed = core::mem::take(&mut self.since_collect);
        let mut sink = CandidateSink {
            channel_id: self.channel_id,
            channel_priority: self.setup.priority,
            candidates: output,
        };
        match &mut self.state {
            SendState::Unreliable(state) => state.collect_candidates(elapsed, &mut sink),
            SendState::Reliable(state) => state.collect_candidates(elapsed, &mut sink),
        }
    }

    /// The pending message that `key` names in this flush.
    pub(crate) fn message(&self, key: SendMessageKey) -> MessageRef<'_> {
        match &self.state {
            SendState::Unreliable(state) => state.message(key),
            SendState::Reliable(state) => state.message(key),
        }
    }

    /// The key of the next due fragment of the message whose fragment `key` names, past it.
    pub(crate) fn next_due(&self, key: SendMessageKey) -> Option<SendMessageKey> {
        match &self.state {
            SendState::Unreliable(state) => state.next_due(key),
            SendState::Reliable(state) => state.next_due(key),
        }
    }

    /// Commits one candidate after the packet containing it enters the link's send queue.
    ///
    /// Unreliable messages are marked for removal at the end of the flush. Reliable messages stay
    /// pending, in flight until the packet session reports their packet's fate. Returns whether a
    /// packet carried the message or fragment before.
    pub(crate) fn commit_send(&mut self, key: SendMessageKey) -> bool {
        self.sent_messages.add(1);
        match &mut self.state {
            SendState::Unreliable(state) => {
                state.commit_send(key);
                false
            }
            SendState::Reliable(state) => state.commit_send(key),
        }
    }

    /// Finalizes channel state after all packets for the current flush have been staged.
    ///
    /// This removes committed unreliable candidates and applies
    /// [`ChannelSetup::retry_unsent_messages`] when the budget prevented eligible unreliable
    /// messages from being sent. Reliable messages remain pending until acknowledged.
    pub(crate) fn end_send(&mut self, outcome: SendFlushOutcome) {
        if let SendState::Unreliable(state) = &mut self.state {
            let discard_unsent = is_ready_to_send(self.timer.as_ref())
                && !self.setup.retry_unsent_messages
                && outcome == SendFlushOutcome::BudgetLimited;
            state.end_send(discard_unsent);
        }
    }

    /// Applies the loss of a packet that carried the part `key` names. An unreliable channel
    /// sends each part once and takes no fate: the transport watches none of its packets.
    pub(crate) fn receive_loss(&mut self, key: SendMessageKey) {
        if let SendState::Reliable(state) = &mut self.state {
            state.receive_loss(key);
        }
    }

    /// Applies an acknowledgement and returns whether it completed the logical message; never on
    /// an unreliable channel, which takes no fate.
    pub(crate) fn receive_ack(&mut self, key: SendMessageKey) -> bool {
        match &mut self.state {
            SendState::Reliable(state) => state.receive_ack(key),
            SendState::Unreliable(_) => false,
        }
    }
}

impl UnreliableSendState {
    const fn new(delivery: UnreliableDelivery) -> Self {
        Self {
            delivery,
            queue: VecDeque::new(),
            queued: 0,
            next_message_id: MessageId(0),
        }
    }

    fn buffer_send(
        &mut self,
        message: Bytes,
        priority: f32,
        fragment_len: usize,
    ) -> Option<MessageId> {
        if self.delivery == UnreliableDelivery::Sequenced {
            // The receiver takes only the newest, so the message supersedes each one not started.
            self.retain(QueuedMessage::is_started);
        }
        let message_id = self.next_message_id;
        let is_fragmented = message.len() > fragment_len;
        let has_id = is_fragmented || self.delivery != UnreliableDelivery::Unordered;

        let message = if is_fragmented {
            QueuedMessage::Fragmented(FragmentedMessage::split(message_id, message, fragment_len))
        } else {
            QueuedMessage::whole(SingleBody::new(has_id.then_some(message_id), message))
        };
        self.queued += message.len();
        self.queue.push_back(Pending::new(message, priority));

        has_id.then(|| {
            self.next_message_id += 1;
            message_id
        })
    }

    /// Appends each message with a due part. A message's send order is its place among the
    /// queue's messages of its shape, as whole messages and fragmented ones compete apart.
    fn collect_candidates(&mut self, elapsed: Duration, sink: &mut CandidateSink<'_>) {
        let elapsed_secs = elapsed.as_secs_f32();
        let mut wholes = 0;
        let mut fragmented = 0;
        for (index, pending) in self.queue.iter_mut().enumerate() {
            let shape_order = if pending.message.is_whole() {
                &mut wholes
            } else {
                &mut fragmented
            };
            let send_order = *shape_order;
            *shape_order += 1;
            let Some(part) = pending.message.first_due() else {
                continue;
            };
            pending.priority += pending.base_priority * elapsed_secs;
            let rank = CandidateRank {
                priority: pending.priority,
                send_order,
            };
            sink.push(
                SendMessageKey::new(u32::try_from(index).unwrap(), part),
                rank,
                pending.message.part(part),
            );
        }
    }

    fn message(&self, key: SendMessageKey) -> MessageRef<'_> {
        self.queue[index_of(key.message)].message.part(key.part)
    }

    fn next_due(&self, key: SendMessageKey) -> Option<SendMessageKey> {
        self.queue[index_of(key.message)]
            .message
            .next_due(key.part + 1)
            .map(|next| key.with_part(next))
    }

    fn commit_send(&mut self, key: SendMessageKey) {
        self.queue[index_of(key.message)].message.commit(key.part);
    }

    /// Drops what the flush sent. With `discard_unsent`, it drops what it did not send too, but
    /// a fragmented message it started, whose fragments the receiver waits for.
    fn end_send(&mut self, discard_unsent: bool) {
        if discard_unsent {
            self.retain(|message| message.is_started() && !message.is_sent());
        } else {
            self.retain(|message| !message.is_sent());
        }
    }

    /// Keeps the messages `keep` takes, and counts the bytes of the others off.
    fn retain(&mut self, keep: impl Fn(&QueuedMessage) -> bool) {
        let queued = &mut self.queued;
        self.queue.retain(|pending| {
            let kept = keep(&pending.message);
            if !kept {
                *queued -= pending.message.len();
            }
            kept
        });
    }
}

impl CandidateMessages for ChannelSlots<ChannelSender> {
    fn message(&self, channel_id: NetId, key: SendMessageKey) -> MessageRef<'_> {
        self[channel_id].message(key)
    }

    fn next_due(&self, channel_id: NetId, key: SendMessageKey) -> Option<SendMessageKey> {
        self[channel_id].next_due(key)
    }
}

/// The queue index a key's slot names.
fn index_of(slot: u32) -> usize {
    usize::try_from(slot).unwrap()
}

fn is_ready_to_send(timer: Option<&SendTimer>) -> bool {
    timer.is_none_or(|timer| timer.fired)
}

/// A channel's send frequency: it fires in each update that completes a period, and keeps the
/// overrun toward the next, as a repeating timer does.
#[derive(Debug, Clone, Copy)]
struct SendTimer {
    period: Duration,
    elapsed: Duration,
    /// Whether the last update completed a period.
    fired: bool,
}

impl SendTimer {
    const fn new(period: Duration) -> Self {
        Self {
            period,
            elapsed: Duration::ZERO,
            fired: false,
        }
    }

    fn tick(&mut self, delta: Duration) {
        self.elapsed += delta;
        self.fired = self.elapsed >= self.period;
        if self.fired {
            let overrun = self.elapsed.as_nanos() % self.period.as_nanos();
            self.elapsed = Duration::from_nanos(
                u64::try_from(overrun).expect("an overrun is below its period"),
            );
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::channel_sender::{ChannelSender, SendState};
    use crate::channel_type_id::ChannelTypeId;
    use crate::message_id::MessageId;

    impl ChannelSender {
        /// The type-level channel key associated with this channel.
        pub(crate) const fn channel_type_id(&self) -> ChannelTypeId {
            self.channel_type_id
        }

        /// The ids of the reliable messages not acknowledged yet; none for an unreliable channel.
        pub(crate) fn pending_reliable(&self) -> Vec<MessageId> {
            match &self.state {
                SendState::Unreliable(_) => Vec::new(),
                SendState::Reliable(state) => state.pending_ids().collect(),
            }
        }
    }
}

#[cfg(test)]
mod tests;
