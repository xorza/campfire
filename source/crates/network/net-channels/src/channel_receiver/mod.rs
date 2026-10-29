//! Channel receive state and delivery policies.
use core::time::Duration;
use std::collections::VecDeque;

use bytes::Bytes;
use campfire_net_core::Instant;
use campfire_net_core::NetTick;

use crate::bench_counter::BenchCounter;
use crate::channel_message::ChannelMessage;
use crate::channel_mode::ChannelMode;
use crate::channel_reader::ChannelReader;
use crate::channel_setup::ChannelSetup;
use crate::channel_type_id::ChannelTypeId;
use crate::error::ReceiveError;
use crate::fragment_body::FragmentBody;
use crate::fragment_receiver::{FragmentCx, FragmentOutcome, FragmentReceiver, PartialExpiry};
use crate::gap_store::GapStore;
use crate::message_id::MessageId;
use crate::receive_budget::{ReceiveBudget, Reservation};
use crate::reliable_send_state::SEND_WINDOW;
use crate::single_body::SingleBody;

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
    /// The layer that reads its messages.
    reader: ChannelReader,
    fragments: FragmentReceiver,
    current_time: Instant,
    state: ReceiveState,
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
        /// The oldest id not received: every id before it was.
        pending: MessageId,
        /// Completed messages in completion order, as required by unordered delivery.
        ready: VecDeque<ChannelMessage>,
        /// The ids past `pending` received, which suppress a copy.
        received: GapStore<MessageId>,
    },
    ReliableOrdered {
        /// The next id to deliver: every id before it is in `ready` or was read.
        pending: MessageId,
        /// The messages before `pending` that were not read, in order.
        ready: VecDeque<ChannelMessage>,
        /// The completed messages past `pending`, each a copy that holds no packet's bytes.
        waiting: GapStore<ChannelMessage>,
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
    /// An id past the window an honest sender keeps ahead of the id this end waits for.
    Violation,
    /// Admitted as the newest message of a sequenced channel, which supersedes every older one.
    AdmitNewest(MessageId),
}

impl ChannelReceiver {
    pub(crate) fn new(
        channel_type_id: ChannelTypeId,
        name: &'static str,
        setup: &ChannelSetup,
        reader: ChannelReader,
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
                received: GapStore::default(),
            },
            ChannelMode::OrderedReliable => ReceiveState::ReliableOrdered {
                pending: MessageId::default(),
                ready: VecDeque::new(),
                waiting: GapStore::default(),
            },
        };
        Self {
            channel_type_id,
            name,
            setup: *setup,
            reader,
            fragments: FragmentReceiver::new(),
            current_time: Instant::ZERO,
            state,
            received_bytes: BenchCounter::default(),
        }
    }

    /// Drops every partial and completed message, as a new receiver with the same setup
    /// starts.
    pub(crate) fn reset(&mut self) {
        *self = Self::new(self.channel_type_id, self.name, &self.setup, self.reader);
    }

    /// The layer that reads its messages (11-net-stack/transport.md, A channel has one reader).
    pub const fn reader(&self) -> ChannelReader {
        self.reader
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

    /// The bytes this channel holds against the receive budget: its partial messages, and what
    /// it holds past a gap. What it delivers, the message plugin reads in the same frame.
    pub(crate) fn held(&self) -> usize {
        self.fragments.held() + self.state.held()
    }

    /// Stores a received whole message, of an id when [`Self::single_has_id`], charging what it
    /// leaves undelivered to `budget`; a message the budget has no room for is not stored, and its
    /// packet must not be acked. A message no honest peer sends breaks the protocol.
    pub(crate) fn receive_single(
        &mut self,
        single: SingleBody,
        tick: NetTick,
        budget: &mut ReceiveBudget,
    ) -> Result<BufferOutcome, ReceiveError> {
        let message_id = single.id;
        match self.state.admit(message_id) {
            MessageAdmission::Refuse => return Ok(BufferOutcome::Stored),
            MessageAdmission::Violation => return Err(ReceiveError::PastWindow),
            MessageAdmission::Admit => {}
            MessageAdmission::AdmitNewest(newest) => self.fragments.discard_older_than(newest),
        }
        let message = ChannelMessage {
            tick,
            bytes: single.bytes,
            message_id,
        };
        let outcome = self
            .state
            .push_completed(message, Reservation::Limited, budget);
        // A partial of its id would never complete: its fragments are copies from now on.
        if outcome == BufferOutcome::Stored
            && let Some(id) = message_id
        {
            self.fragments.remove(id);
        }
        Ok(outcome)
    }

    /// Stores a received fragment, as [`Self::receive_single`] stores a whole message; a
    /// fragment no honest peer sends breaks the protocol.
    pub(crate) fn receive_fragment(
        &mut self,
        fragment: FragmentBody,
        tick: NetTick,
        budget: &mut ReceiveBudget,
    ) -> Result<BufferOutcome, ReceiveError> {
        let message_id = Some(fragment.message_id);
        match self.state.admit(message_id) {
            MessageAdmission::Refuse => return Ok(BufferOutcome::Stored),
            MessageAdmission::Violation => return Err(ReceiveError::PastWindow),
            MessageAdmission::Admit => {}
            MessageAdmission::AdmitNewest(newest) => self.fragments.discard_older_than(newest),
        }
        let cx = FragmentCx {
            expiry: if self.state.expires_fragments() {
                PartialExpiry::Since(self.current_time)
            } else {
                PartialExpiry::Never
            },
            max_message_len: self.setup.max_message_len,
            reservation: if self.state.is_head(message_id) {
                Reservation::Head
            } else {
                Reservation::Limited
            },
        };
        match self
            .fragments
            .receive_fragment(fragment, tick, &cx, budget)?
        {
            FragmentOutcome::Pending => Ok(BufferOutcome::Stored),
            FragmentOutcome::OverBudget => Ok(BufferOutcome::OverBudget),
            // Its reassembly buffer was already charged: the message is kept whatever room is
            // left.
            FragmentOutcome::Complete { tick, bytes } => {
                let message = ChannelMessage {
                    tick,
                    bytes,
                    message_id,
                };
                Ok(self
                    .state
                    .push_completed(message, Reservation::Head, budget))
            }
        }
    }

    /// Removes and returns the next message accepted by the channel's delivery policy.
    pub fn read_message(&mut self) -> Option<ChannelMessage> {
        self.state.read_message()
    }
}

impl ReceiveState {
    /// Only an unreliable unordered channel expires partial messages by time. A reliable
    /// channel's acked fragments are never resent, so expiring its partial would lose the
    /// message; a sequenced channel drops a partial once a newer message supersedes it.
    const fn expires_fragments(&self) -> bool {
        matches!(self, Self::UnreliableUnordered { .. })
    }

    /// The bytes the state holds past a gap.
    fn held(&self) -> usize {
        match self {
            Self::ReliableUnordered { received, .. } => received.held(),
            Self::ReliableOrdered { waiting, .. } => waiting.held(),
            Self::UnreliableUnordered { .. } | Self::Sequenced { .. } => 0,
        }
    }

    /// Whether `message_id` is the message an ordered channel delivers next.
    fn is_head(&self, message_id: Option<MessageId>) -> bool {
        matches!(self, Self::ReliableOrdered { pending, .. } if Some(*pending) == message_id)
    }

    /// Whether a reliable message of `message_id` enters a channel that waits for `pending`: one
    /// before it, or `held`, came already; one at a distance of `SEND_WINDOW` or more is a claim
    /// no honest sender makes, as its window of unacked ids starts at or before `pending`.
    fn reliable(
        pending: MessageId,
        message_id: MessageId,
        held: impl FnOnce(MessageId) -> bool,
    ) -> MessageAdmission {
        let distance = message_id - pending;
        if distance < 0 || held(message_id) {
            MessageAdmission::Refuse
        } else if distance.unsigned_abs() >= SEND_WINDOW {
            MessageAdmission::Violation
        } else {
            MessageAdmission::Admit
        }
    }

    /// Whether a message of `message_id` enters, which a channel of ids always has.
    fn admit(&mut self, message_id: Option<MessageId>) -> MessageAdmission {
        match self {
            Self::UnreliableUnordered { .. } => MessageAdmission::Admit,
            Self::ReliableUnordered {
                pending, received, ..
            } => {
                let message_id = message_id.expect("a reliable message has an id");
                Self::reliable(*pending, message_id, |id| received.contains(id))
            }
            Self::ReliableOrdered {
                pending, waiting, ..
            } => {
                let message_id = message_id.expect("a reliable message has an id");
                Self::reliable(*pending, message_id, |id| waiting.contains(id))
            }
            Self::Sequenced {
                ready,
                newest_seen,
                newest_completed,
            } => {
                let message_id = message_id.expect("a sequenced message has an id");
                if *newest_completed == Some(message_id) {
                    return MessageAdmission::Refuse;
                }
                match *newest_seen {
                    Some(newest) if message_id - newest < 0 => MessageAdmission::Refuse,
                    Some(newest) if message_id == newest => MessageAdmission::Admit,
                    _ => {
                        *newest_seen = Some(message_id);
                        // A newer message supersedes an older completed one even if the
                        // application has not drained it yet.
                        *ready = None;
                        MessageAdmission::AdmitNewest(message_id)
                    }
                }
            }
        }
    }

    /// Stores a completed `message` that its admission let in; a message past a gap is charged
    /// as `reservation` asks, and not stored when the budget refuses it.
    fn push_completed(
        &mut self,
        message: ChannelMessage,
        reservation: Reservation,
        budget: &mut ReceiveBudget,
    ) -> BufferOutcome {
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
                let message_id = message.message_id.expect("sequenced messages have ids");
                if *newest_completed == Some(message_id)
                    || newest_seen.is_some_and(|newest| message_id - newest < 0)
                {
                    return BufferOutcome::Stored;
                }
                *newest_completed = Some(message_id);
                *ready = Some(message);
            }
            Self::ReliableUnordered {
                pending,
                ready,
                received,
            } => {
                let message_id = message.message_id.expect("reliable messages have ids");
                if message_id == *pending {
                    *pending += 1;
                    while received.pop(*pending).is_some() {
                        *pending += 1;
                    }
                } else if !received.insert(message_id, reservation, budget) {
                    return BufferOutcome::OverBudget;
                }
                ready.push_back(message);
            }
            Self::ReliableOrdered {
                pending,
                ready,
                waiting,
            } => {
                let message_id = message.message_id.expect("reliable messages have ids");
                if message_id != *pending {
                    let held = ChannelMessage {
                        bytes: Bytes::copy_from_slice(&message.bytes),
                        ..message
                    };
                    return if waiting.insert(held, reservation, budget) {
                        BufferOutcome::Stored
                    } else {
                        BufferOutcome::OverBudget
                    };
                }
                ready.push_back(message);
                *pending += 1;
                while let Some(next) = waiting.pop(*pending) {
                    ready.push_back(next);
                    *pending += 1;
                }
            }
        }
        BufferOutcome::Stored
    }

    fn read_message(&mut self) -> Option<ChannelMessage> {
        match self {
            Self::UnreliableUnordered { ready } => ready.pop_front(),
            Self::Sequenced { ready, .. } => ready.take(),
            Self::ReliableUnordered { ready, .. } | Self::ReliableOrdered { ready, .. } => {
                ready.pop_front()
            }
        }
    }
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
