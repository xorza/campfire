use bytes::Bytes;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;

use crate::message_id::MessageId;

/// A message too large for one packet: its bytes once, which each fragment slices at its index,
/// and the state of each fragment.
///
/// Its fragment size is the one its split used, so a link that sizes its fragments again does
/// not change a message whose fragments the receiver may hold.
#[derive(Debug)]
pub(crate) struct FragmentedMessage {
    message_id: MessageId,
    bytes: Bytes,
    fragment_len: usize,
    states: Vec<PartState>,
    /// No fragment before it is due.
    due_from: u16,
    /// The fragments a packet carried at least once.
    sent: u16,
    acked: u16,
}

/// Where a part of a message is, a whole message's or a fragment: it goes in the next packet that
/// has room while it is due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartState {
    Unsent,
    /// A packet that carries it went to the packet session, and its fate is not known: on an
    /// unreliable channel, for good.
    InFlight,
    /// The packet session found the packet that carried it lost.
    Lost,
    Acked,
}

impl PartState {
    pub(crate) const fn is_due(self) -> bool {
        matches!(self, Self::Unsent | Self::Lost)
    }
}

impl FragmentedMessage {
    /// Splits `bytes`, which is longer than `fragment_len`, into fragments of that size.
    pub(crate) fn split(message_id: MessageId, bytes: Bytes, fragment_len: usize) -> Self {
        debug_assert!(
            bytes.len() > fragment_len,
            "a message of {} bytes fits a fragment of {fragment_len}",
            bytes.len()
        );
        let fragments = u16::try_from(bytes.len().div_ceil(fragment_len))
            .expect("a channel's message bound keeps its fragments under 2^16");
        Self {
            message_id,
            bytes,
            fragment_len,
            states: vec![PartState::Unsent; usize::from(fragments)],
            due_from: 0,
            sent: 0,
            acked: 0,
        }
    }

    pub(crate) const fn set_message_id(&mut self, message_id: MessageId) {
        self.message_id = message_id;
    }

    /// The bytes of the message.
    pub(crate) const fn len(&self) -> usize {
        self.bytes.len()
    }

    pub(crate) fn fragments(&self) -> u16 {
        u16::try_from(self.states.len()).unwrap()
    }

    pub(crate) fn fragment(&self, index: u16) -> FragmentRef<'_> {
        debug_assert!(index < self.fragments(), "fragment {index} of {self:?}");
        FragmentRef {
            message: self,
            index,
        }
    }

    /// The first due fragment at or after `from`.
    pub(crate) fn next_due(&self, from: u16) -> Option<u16> {
        let start = usize::from(from).min(self.states.len());
        self.states[start..]
            .iter()
            .position(|state| state.is_due())
            .map(|offset| from + u16::try_from(offset).unwrap())
    }

    /// The first due fragment, which moves the start of the next search to it.
    pub(crate) fn first_due(&mut self) -> Option<u16> {
        let first = self.next_due(self.due_from);
        self.due_from = first.unwrap_or_else(|| self.fragments());
        first
    }

    /// Records that a packet carries fragment `index`, and returns whether one carried it before
    /// and was lost.
    pub(crate) fn commit(&mut self, index: u16) -> bool {
        let state = &mut self.states[usize::from(index)];
        let previous = core::mem::replace(state, PartState::InFlight);
        if previous == PartState::Unsent {
            self.sent += 1;
        }
        previous == PartState::Lost
    }

    /// Applies the loss of the packet that carried fragment `index`, which is due again unless
    /// it was acknowledged since.
    pub(crate) fn lose(&mut self, index: u16) {
        let state = &mut self.states[usize::from(index)];
        if *state == PartState::InFlight {
            *state = PartState::Lost;
            self.due_from = self.due_from.min(index);
        }
    }

    /// Applies the acknowledgement of fragment `index`, and returns whether it completed the
    /// message.
    pub(crate) fn ack(&mut self, index: u16) -> bool {
        let state = &mut self.states[usize::from(index)];
        if *state == PartState::Acked {
            return false;
        }
        *state = PartState::Acked;
        self.acked += 1;
        self.acked == self.fragments()
    }

    /// Whether a packet carried one of its fragments.
    pub(crate) const fn is_started(&self) -> bool {
        self.sent > 0
    }

    /// Whether a packet carried each of its fragments.
    pub(crate) fn is_sent(&self) -> bool {
        self.sent == self.fragments()
    }
}

/// One fragment of a [`FragmentedMessage`], as a packet writes it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FragmentRef<'a> {
    message: &'a FragmentedMessage,
    index: u16,
}

impl<'a> FragmentRef<'a> {
    pub(crate) fn is_last(self) -> bool {
        self.index + 1 == self.message.fragments()
    }

    /// The message's bytes this fragment carries.
    pub(crate) fn payload(self) -> &'a [u8] {
        let start = usize::from(self.index) * self.message.fragment_len;
        let end = (start + self.message.fragment_len).min(self.message.bytes.len());
        &self.message.bytes[start..end]
    }

    /// Writes the fragment as [`FragmentBody`](crate::fragment_body::FragmentBody) reads it: the
    /// message id, the fragment's index and count, and its bytes.
    pub(crate) fn write(self, buffer: &mut impl WriteInteger) {
        self.message.message_id.to_bytes(buffer);
        buffer.write_varint(u64::from(self.index));
        buffer.write_varint(u64::from(self.message.fragments()));
        let payload = self.payload();
        buffer.write_varint(payload.len() as u64);
        buffer.write_bytes(payload);
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::fragment_body::FragmentBody;
    use crate::fragment_count::FragmentCount;
    use crate::fragment_index::FragmentIndex;
    use crate::fragmented_message::{FragmentRef, FragmentedMessage};
    use bytes::Bytes;

    impl FragmentedMessage {
        /// Each fragment as the receiver reads it.
        pub(crate) fn fragment_body(&self) -> Vec<FragmentBody> {
            (0..self.fragments())
                .map(|index| self.fragment(index).data())
                .collect()
        }
    }

    impl FragmentRef<'_> {
        /// The fragment as the receiver reads it.
        pub(crate) fn data(self) -> FragmentBody {
            FragmentBody {
                message_id: self.message.message_id,
                fragment_index: FragmentIndex(u64::from(self.index)),
                fragments: FragmentCount(u64::from(self.message.fragments())),
                bytes: Bytes::copy_from_slice(self.payload()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fragment_body::FragmentBody;
    use crate::fragment_count::FragmentCount;
    use crate::fragment_index::FragmentIndex;
    use crate::packet::FRAGMENT_LEN;
    use campfire_net_bytes::Reader;
    use campfire_net_bytes::Writer;

    fn split(len: usize) -> FragmentedMessage {
        FragmentedMessage::split(
            MessageId(7),
            Bytes::from((0..len).map(|byte| byte as u8).collect::<Vec<_>>()),
            FRAGMENT_LEN,
        )
    }

    /// 2.5 fragments of bytes make 3 fragments, each a slice at its index; each writes what
    /// `FragmentBody` reads back.
    #[test]
    fn each_fragment_writes_what_the_receiver_reads() {
        let len = FRAGMENT_LEN * 5 / 2;
        let message = split(len);
        assert_eq!(message.fragments(), 3);
        let bytes = Bytes::from((0..len).map(|byte| byte as u8).collect::<Vec<_>>());
        for (index, range) in [
            (0, 0..FRAGMENT_LEN),
            (1, FRAGMENT_LEN..2 * FRAGMENT_LEN),
            (2, 2 * FRAGMENT_LEN..len),
        ] {
            let fragment = message.fragment(index);
            assert_eq!(fragment.is_last(), index == 2);
            let mut writer = Writer::default();
            fragment.write(&mut writer);
            let written = writer.take_written();
            let read = FragmentBody::from_bytes(&mut Reader::from(written)).unwrap();
            assert_eq!(
                read,
                FragmentBody {
                    message_id: MessageId(7),
                    fragment_index: FragmentIndex(u64::from(index)),
                    fragments: FragmentCount(3),
                    bytes: bytes.slice(range),
                }
            );
        }
    }

    /// Of 3 fragments, the commit of 0 and 2 leaves 1 due; the loss of 2 makes it due again and
    /// its next commit a resend; an ack of each completes the message once, at the third.
    #[test]
    fn a_fragment_is_due_until_sent_and_again_once_lost() {
        let mut message = split(FRAGMENT_LEN * 5 / 2);
        assert_eq!(message.first_due(), Some(0));
        assert!(!message.commit(0));
        assert!(!message.commit(2));
        assert!(message.is_started() && !message.is_sent());
        assert_eq!(message.first_due(), Some(1));
        assert_eq!(message.next_due(2), None);
        assert!(!message.commit(1));
        assert!(message.is_sent());
        assert_eq!(message.first_due(), None);

        message.lose(2);
        assert_eq!(message.first_due(), Some(2));
        assert!(message.commit(2));
        assert!(!message.ack(0));
        assert!(!message.ack(0));
        assert!(!message.ack(1));
        message.lose(1);
        assert_eq!(message.first_due(), None, "an acked fragment is not lost");
        assert!(message.ack(2));
    }
}
