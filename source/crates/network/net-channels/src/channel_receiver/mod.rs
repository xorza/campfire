//! Channel receive state and delivery policies.

use crate::channel_message::ChannelMessage;
use bytes::Bytes;
use campfire_net_core::Instant;
use campfire_net_core::NetTick;
use core::time::Duration;
use std::collections::VecDeque;

use crate::bench_counter::BenchCounter;
use crate::channel_mode::ChannelMode;
use crate::channel_setup::ChannelSetup;
use crate::channel_type_id::ChannelTypeId;
use crate::message_body::MessageBody;
use crate::message_id::MessageId;
use crate::receive_message::ReceiveMessage;

use crate::channel_receive_error::ChannelReceiveError;
use crate::fragment_receiver::{
    FragmentCx, FragmentOutcome, FragmentReceiver, PartialExpiry, Reservation,
};
use crate::receive_budget::ReceiveBudget;

type Result<T> = core::result::Result<T, ChannelReceiveError>;

const DISCARD_AFTER: Duration = Duration::from_millis(3000);

/// The receiving half of one registered transport channel.
///
/// Fragment reassembly and time bookkeeping are shared by every delivery mode. Only the policy
/// for accepting and exposing completed messages varies through the private state enum.
#[derive(Debug)]
pub struct ChannelReceiver {
    channel_type_id: ChannelTypeId,
    /// The channel's type name, for the traces of what it receives.
    name: &'static str,
    setup: ChannelSetup,
    fragments: FragmentReceiver,
    current_time: Instant,
    state: ReceiveState,
    /// The bytes of the ordered messages waiting behind a gap, as charged to the receive budget.
    ready_held: usize,
    /// Bytes of this channel's messages parsed from received packets.
    pub(crate) received_bytes: BenchCounter,
}

#[derive(Debug)]
enum ReceiveState {
    UnreliableUnordered {
        ready: VecDeque<ChannelMessage>,
    },
    /// Both sequenced modes: only the newest message is delivered, so an older one, or another
    /// copy of the newest completed one, is dropped on arrival.
    Sequenced {
        ready: Option<ChannelMessage>,
        newest_seen: Option<MessageId>,
        /// Outlives `ready`: taking a message for delivery must not make a later copy of it,
        /// a retransmission or a duplicated packet, eligible again.
        newest_completed: Option<MessageId>,
    },
    ReliableUnordered {
        /// Oldest message ID not yet covered by the contiguous received prefix.
        pending: MessageId,
        /// Completed messages in completion order, as required by unordered delivery.
        ready: VecDeque<ChannelMessage>,
        /// Whether each id from `pending` on, at its distance from it, was received, which
        /// suppresses a copy.
        received: VecDeque<bool>,
    },
    ReliableOrdered {
        /// Next message ID eligible for delivery.
        pending: MessageId,
        /// The completed messages from `pending` on, at their id's distance from it.
        ready: VecDeque<Option<ChannelMessage>>,
    },
}

/// Whether a received message was stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BufferOutcome {
    /// Stored, or refused as a copy or as older than the channel delivers: either way the packet
    /// may be accepted.
    Stored,
    /// The receive budget had no room for it; its packet is not accepted, so it gets no ACK.
    OverBudget,
}

/// Whether a received message enters the channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessageAdmission {
    Refuse,
    Admit,
    /// Admitted as the newest message of a sequenced channel, which supersedes every older one.
    AdmitNewest(MessageId),
}

impl ChannelReceiver {
    pub(crate) fn new(
        channel_type_id: ChannelTypeId,
        name: &'static str,
        setup: &ChannelSetup,
    ) -> Self {
        let state = match setup.mode {
            ChannelMode::UnorderedUnreliable => ReceiveState::UnreliableUnordered {
                ready: VecDeque::new(),
            },
            ChannelMode::SequencedUnreliable | ChannelMode::SequencedReliable => {
                ReceiveState::Sequenced {
                    ready: None,
                    newest_seen: None,
                    newest_completed: None,
                }
            }
            ChannelMode::UnorderedReliable => ReceiveState::ReliableUnordered {
                pending: MessageId::default(),
                ready: VecDeque::new(),
                received: VecDeque::new(),
            },
            ChannelMode::OrderedReliable => ReceiveState::ReliableOrdered {
                pending: MessageId::default(),
                ready: VecDeque::new(),
            },
        };
        Self {
            channel_type_id,
            name,
            setup: *setup,
            fragments: FragmentReceiver::new(),
            current_time: Instant::ZERO,
            state,
            ready_held: 0,
            received_bytes: BenchCounter::default(),
        }
    }

    /// Drops every partial and completed message, as a new receiver with the same setup
    /// starts.
    pub(crate) fn reset(&mut self) {
        *self = Self::new(self.channel_type_id, self.name, &self.setup);
    }

    /// The type-level channel key associated with this channel.
    pub const fn channel_type_id(&self) -> ChannelTypeId {
        self.channel_type_id
    }

    /// The channel's type name.
    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub(crate) fn update(&mut self, now: Instant) {
        self.current_time = now;
        if self.state.expires_fragments() {
            self.fragments
                .cleanup(self.current_time.saturating_sub(DISCARD_AFTER));
        }
    }

    /// Whether each single message of this channel carries its id.
    pub(crate) const fn single_has_id(&self) -> bool {
        self.setup.mode.single_has_id()
    }

    /// The largest message this channel receives, in uncompressed bytes.
    pub(crate) const fn max_message_len(&self) -> usize {
        self.setup.max_message_len
    }

    /// The bytes this channel holds against the receive budget.
    pub(crate) const fn held(&self) -> usize {
        self.fragments.held() + self.ready_held
    }

    /// Stores a received message, charging what it leaves undelivered to `budget`; a message the
    /// budget has no room for is not stored, and its packet must not be acked.
    pub(crate) fn buffer_receive(
        &mut self,
        message: ReceiveMessage,
        budget: &mut ReceiveBudget,
    ) -> Result<BufferOutcome> {
        let message_id = message.data.message_id();
        match self.state.admit(message_id)? {
            MessageAdmission::Refuse => return Ok(BufferOutcome::Stored),
            MessageAdmission::Admit => {}
            MessageAdmission::AdmitNewest(newest) => self.fragments.discard_older_than(newest),
        }
        let head = self.state.is_head(message_id);

        let (tick, bytes) = match message.data {
            MessageBody::Single(single) => (message.remote_sent_tick, single.bytes),
            MessageBody::Fragment(fragment) => {
                let cx = FragmentCx {
                    expiry: if self.state.expires_fragments() {
                        PartialExpiry::Since(self.current_time)
                    } else {
                        PartialExpiry::Never
                    },
                    compression: message.compression,
                    max_message_len: self.setup.max_message_len,
                    reservation: if head {
                        Reservation::Head
                    } else {
                        Reservation::Limited
                    },
                };
                match self.fragments.receive_fragment(
                    fragment,
                    message.remote_sent_tick,
                    &cx,
                    budget,
                )? {
                    FragmentOutcome::Pending => return Ok(BufferOutcome::Stored),
                    FragmentOutcome::OverBudget => return Ok(BufferOutcome::OverBudget),
                    // Its reassembly buffer was already charged: the message is kept whatever
                    // room is left.
                    FragmentOutcome::Complete { tick, bytes } => {
                        if self.state.waits_behind_a_gap() {
                            budget.charge(bytes.len());
                            self.ready_held += bytes.len();
                        }
                        self.state.push_completed(message_id, tick, bytes);
                        return Ok(BufferOutcome::Stored);
                    }
                }
            }
        };
        if self.state.waits_behind_a_gap() {
            if head {
                budget.charge(bytes.len());
            } else if !budget.try_charge(bytes.len()) {
                return Ok(BufferOutcome::OverBudget);
            }
            self.ready_held += bytes.len();
        }
        self.state.push_completed(message_id, tick, bytes);
        Ok(BufferOutcome::Stored)
    }

    /// Removes and returns the next message accepted by the channel's delivery policy.
    pub fn read_message(&mut self) -> Option<ChannelMessage> {
        let message = self.state.read_message()?;
        if self.state.waits_behind_a_gap() {
            self.ready_held -= message.bytes.len();
        }
        Some(message)
    }
}

impl ReceiveState {
    /// Only an unreliable unordered channel expires partial messages by time. A reliable
    /// channel's acked fragments are never resent, so expiring its partial would lose the
    /// message; a sequenced channel drops a partial once a newer message supersedes it.
    const fn expires_fragments(&self) -> bool {
        matches!(self, Self::UnreliableUnordered { .. })
    }

    /// Whether completed messages wait for the ones before them, and so hold budget. Every other
    /// mode exposes a completed message at once, and the message plugin drains it the same frame.
    const fn waits_behind_a_gap(&self) -> bool {
        matches!(self, Self::ReliableOrdered { .. })
    }

    /// Whether `message_id` is the message an ordered channel delivers next.
    fn is_head(&self, message_id: Option<MessageId>) -> bool {
        matches!(self, Self::ReliableOrdered { pending, .. } if Some(*pending) == message_id)
    }

    fn admit(&mut self, message_id: Option<MessageId>) -> Result<MessageAdmission> {
        let admitted = |admit: bool| {
            if admit {
                MessageAdmission::Admit
            } else {
                MessageAdmission::Refuse
            }
        };
        match self {
            Self::UnreliableUnordered { .. } => Ok(MessageAdmission::Admit),
            Self::ReliableUnordered {
                pending, received, ..
            } => {
                let message_id = message_id.ok_or(ChannelReceiveError::MissingMessageId)?;
                Ok(admitted(distance(*pending, message_id).is_some_and(
                    |distance| !received.get(distance).copied().unwrap_or(false),
                )))
            }
            Self::ReliableOrdered { pending, ready } => {
                let message_id = message_id.ok_or(ChannelReceiveError::MissingMessageId)?;
                Ok(admitted(distance(*pending, message_id).is_some_and(
                    |distance| ready.get(distance).is_none_or(Option::is_none),
                )))
            }
            Self::Sequenced {
                ready,
                newest_seen,
                newest_completed,
            } => {
                let message_id = message_id.ok_or(ChannelReceiveError::MissingMessageId)?;
                if *newest_completed == Some(message_id) {
                    return Ok(MessageAdmission::Refuse);
                }
                match *newest_seen {
                    Some(newest) if message_id - newest < 0 => Ok(MessageAdmission::Refuse),
                    Some(newest) if message_id == newest => Ok(MessageAdmission::Admit),
                    _ => {
                        *newest_seen = Some(message_id);
                        // A newer message supersedes an older completed one even if the
                        // application has not drained it yet.
                        *ready = None;
                        Ok(MessageAdmission::AdmitNewest(message_id))
                    }
                }
            }
        }
    }

    fn push_completed(&mut self, message_id: Option<MessageId>, tick: NetTick, bytes: Bytes) {
        let message = ChannelMessage {
            tick,
            bytes,
            message_id,
        };
        match self {
            Self::UnreliableUnordered { ready } => ready.push_back(ChannelMessage {
                message_id: None,
                ..message
            }),
            Self::Sequenced {
                ready,
                newest_seen,
                newest_completed,
            } => {
                let message_id = message_id.expect("sequenced messages have ids");
                if *newest_completed == Some(message_id)
                    || newest_seen.is_some_and(|newest| message_id - newest < 0)
                {
                    return;
                }
                *newest_completed = Some(message_id);
                *ready = Some(message);
            }
            Self::ReliableUnordered {
                pending,
                ready,
                received,
            } => {
                let message_id = message_id.expect("reliable messages have ids");
                *slot(received, *pending, message_id) = true;
                ready.push_back(message);
            }
            Self::ReliableOrdered { pending, ready } => {
                let message_id = message_id.expect("reliable messages have ids");
                *slot(ready, *pending, message_id) = Some(message);
            }
        }
    }

    fn read_message(&mut self) -> Option<ChannelMessage> {
        match self {
            Self::UnreliableUnordered { ready } => ready.pop_front(),
            Self::Sequenced { ready, .. } => ready.take(),
            Self::ReliableUnordered {
                pending,
                ready,
                received,
            } => {
                let message = ready.pop_front()?;
                if message.message_id == Some(*pending) {
                    while received.front() == Some(&true) {
                        received.pop_front();
                        *pending += 1;
                    }
                }
                Some(message)
            }
            Self::ReliableOrdered { pending, ready } => {
                let message = ready.front_mut()?.take()?;
                ready.pop_front();
                *pending += 1;
                Some(message)
            }
        }
    }
}

/// The distance of `message_id` from `pending`, if it is not older: a reliable receiver takes an
/// id up to 32,767 past its oldest undelivered one as new.
fn distance(pending: MessageId, message_id: MessageId) -> Option<usize> {
    usize::try_from(message_id - pending).ok()
}

/// The entry of `message_id`, which is not older than `pending`, in a ring that starts at
/// `pending`, with the ring grown to hold it.
fn slot<T: Default>(ring: &mut VecDeque<T>, pending: MessageId, message_id: MessageId) -> &mut T {
    let distance = distance(pending, message_id).expect("an admitted id is not older");
    if ring.len() <= distance {
        ring.resize_with(distance + 1, T::default);
    }
    &mut ring[distance]
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::channel_receiver::ChannelReceiver;

    impl ChannelReceiver {
        /// Fragmented messages with some fragments still missing.
        pub(crate) fn partial_fragments(&self) -> usize {
            self.fragments.partial_len()
        }
    }
}

#[cfg(test)]
mod tests;
