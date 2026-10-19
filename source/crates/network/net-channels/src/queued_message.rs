use crate::fragmented_message::{FragmentedMessage, PartState};
use crate::message_id::MessageId;
use crate::message_ref::MessageRef;
use crate::single_data::SingleData;

/// A message a channel holds until it is sent, or on a reliable channel acknowledged, as a run of
/// parts: one that fits a packet is one part, a fragmented one a part for each fragment, and each
/// part is due, in flight or acknowledged. A key names a part, so no key names a shape the
/// message lacks (14-workspace-alignment.md, A state the code cannot reach is a type).
#[derive(Debug)]
pub(crate) enum QueuedMessage {
    Whole { data: SingleData, state: PartState },
    Fragmented(FragmentedMessage),
}

impl QueuedMessage {
    pub(crate) const fn whole(data: SingleData) -> Self {
        Self::Whole {
            data,
            state: PartState::Unsent,
        }
    }

    pub(crate) const fn set_id(&mut self, id: MessageId) {
        match self {
            Self::Whole { data, .. } => data.id = Some(id),
            Self::Fragmented(message) => message.set_message_id(id),
        }
    }

    pub(crate) const fn is_whole(&self) -> bool {
        matches!(self, Self::Whole { .. })
    }

    /// Part `part`, as a packet writes it: the whole message or one fragment.
    pub(crate) fn part(&self, part: u16) -> MessageRef<'_> {
        match self {
            Self::Whole { data, .. } => {
                debug_assert_eq!(part, 0, "a whole message is its part 0");
                MessageRef::Single(data)
            }
            Self::Fragmented(message) => MessageRef::Fragment(message.fragment(part)),
        }
    }

    /// The first due part, which moves the start of the next search to it.
    pub(crate) fn first_due(&mut self) -> Option<u16> {
        match self {
            Self::Whole { state, .. } => state.is_due().then_some(0),
            Self::Fragmented(message) => message.first_due(),
        }
    }

    /// The first due part at or after `from`.
    pub(crate) fn next_due(&self, from: u16) -> Option<u16> {
        match self {
            Self::Whole { state, .. } => (from == 0 && state.is_due()).then_some(0),
            Self::Fragmented(message) => message.next_due(from),
        }
    }

    /// Records that a packet carries part `part`, and returns whether one carried it before and
    /// was lost.
    pub(crate) fn commit(&mut self, part: u16) -> bool {
        match self {
            Self::Whole { state, .. } => {
                core::mem::replace(state, PartState::InFlight) == PartState::Lost
            }
            Self::Fragmented(message) => message.commit(part),
        }
    }

    /// Applies the loss of the packet that carried part `part`, which is due again unless it was
    /// acknowledged since.
    pub(crate) fn lose(&mut self, part: u16) {
        match self {
            Self::Whole { state, .. } => {
                if *state == PartState::InFlight {
                    *state = PartState::Lost;
                }
            }
            Self::Fragmented(message) => message.lose(part),
        }
    }

    /// Applies the acknowledgement of part `part`, and returns whether it completed the message.
    pub(crate) fn ack(&mut self, part: u16) -> bool {
        match self {
            Self::Whole { state, .. } => {
                core::mem::replace(state, PartState::Acked) != PartState::Acked
            }
            Self::Fragmented(message) => message.ack(part),
        }
    }

    /// Whether a packet carried one of its parts.
    pub(crate) const fn is_started(&self) -> bool {
        match self {
            Self::Whole { state, .. } => !matches!(state, PartState::Unsent),
            Self::Fragmented(message) => message.is_started(),
        }
    }

    /// Whether a packet carried each of its parts.
    pub(crate) fn is_sent(&self) -> bool {
        match self {
            Self::Whole { state, .. } => *state != PartState::Unsent,
            Self::Fragmented(message) => message.is_sent(),
        }
    }
}
