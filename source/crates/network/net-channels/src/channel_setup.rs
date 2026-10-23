use crate::channel_mode::ChannelMode;
use crate::packet::{FRAGMENT_LEN, MAX_FRAGMENTS};
use crate::receive_budget::DEFAULT_RECEIVE_BUDGET;
use campfire_net_core::RegistryHasher;
use core::time::Duration;

/// How a channel delivers: its mode, direction and send interval.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelSetup {
    /// The ordering and reliability guarantees of the channel.
    pub mode: ChannelMode,
    /// How often should we try to send messages on this channel.
    /// Set to `Duration::default()` to send messages every frame if possible.
    pub send_frequency: Duration,
    /// The channel's weight when the packet session's budget cannot send every message: the
    /// messages go from the highest message priority times channel priority down.
    pub priority: f32,
    /// Whether unreliable messages that the packet session's budget left unsent should remain
    /// queued for a later send flush.
    ///
    /// This only controls what the budget holds back. Once a message goes to the packet session,
    /// its retransmission is determined by [`ChannelMode`]. Reliable channels always retain locally
    /// unsent messages regardless of this setting. If any fragment of an unreliable message has
    /// already been admitted, its remaining fragments are also retained to avoid guaranteeing an
    /// incomplete local send.
    pub retry_unsent_messages: bool,
    /// The largest message this channel carries.
    ///
    /// A larger send is refused with [`SendError::MessageTooLarge`]; a peer whose fragments
    /// claim a larger message breaks the protocol. It bounds what one partial message makes the
    /// receiver allocate.
    ///
    /// [`SendError::MessageTooLarge`]: crate::error::SendError::MessageTooLarge
    pub max_message_len: usize,
}

/// The default [`ChannelSetup::max_message_len`], 512 KiB, which GameNetworkingSockets'
/// `RecvMaxMessageSize` also uses.
pub(crate) const DEFAULT_MAX_MESSAGE_LEN: usize = 512 * 1024;

// A message the receive budget holds goes in `MAX_FRAGMENTS` fragments of a link of the default
// payload, so the budget bounds a channel's message alone.
const _: () = assert!(DEFAULT_RECEIVE_BUDGET <= MAX_FRAGMENTS * FRAGMENT_LEN);

impl ChannelSetup {
    /// Asserts that the receive budget holds this channel's largest message and a fragment more,
    /// which the buffer of a partial message can reserve past it, as a registry's seal checks
    /// each channel (11-net-stack/transport.md, Each link's `Channels` has one receive budget).
    pub fn assert_bounds(&self) {
        assert!(
            self.max_message_len.saturating_add(FRAGMENT_LEN) <= DEFAULT_RECEIVE_BUDGET,
            "a channel's {}-byte message and a {FRAGMENT_LEN}-byte fragment pass the receive \
             budget of {DEFAULT_RECEIVE_BUDGET} bytes",
            self.max_message_len
        );
    }

    /// Adds this setup to a registry's hash, so peers whose channel setups differ fail the protocol
    /// check instead of every packet after it.
    pub fn hash_into(&self, hasher: &mut RegistryHasher) {
        let mode: u8 = match self.mode {
            ChannelMode::UnorderedUnreliable => 0,
            ChannelMode::SequencedUnreliable => 1,
            ChannelMode::UnorderedReliable => 2,
            ChannelMode::SequencedReliable => 3,
            ChannelMode::OrderedReliable => 4,
        };
        hasher.write(&[mode]);
        hasher.write(&duration_bytes(self.send_frequency));
        hasher.write(&self.priority.to_le_bytes());
        hasher.write(&[u8::from(self.retry_unsent_messages)]);
        hasher.write(&u64::try_from(self.max_message_len).unwrap().to_le_bytes());
    }
}

fn duration_bytes(duration: Duration) -> [u8; 12] {
    let mut bytes = [0; 12];
    bytes[..8].copy_from_slice(&duration.as_secs().to_le_bytes());
    bytes[8..].copy_from_slice(&duration.subsec_nanos().to_le_bytes());
    bytes
}

impl Default for ChannelSetup {
    fn default() -> Self {
        Self {
            mode: ChannelMode::UnorderedUnreliable,
            send_frequency: Duration::default(),
            priority: 1.0,
            retry_unsent_messages: true,
            max_message_len: DEFAULT_MAX_MESSAGE_LEN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(max_message_len: usize) -> ChannelSetup {
        ChannelSetup {
            max_message_len,
            ..ChannelSetup::default()
        }
    }

    /// The default 512 KiB message and its fragment fit the 1 MiB budget, as does the largest
    /// message that leaves a fragment's room.
    #[test]
    fn a_message_and_a_fragment_within_the_budget_hold() {
        ChannelSetup::default().assert_bounds();
        setup(DEFAULT_RECEIVE_BUDGET - FRAGMENT_LEN).assert_bounds();
    }

    /// One byte more than the budget less a fragment fails the bounds.
    #[test]
    #[should_panic(expected = "pass the receive budget")]
    fn a_message_and_a_fragment_past_the_budget_fail() {
        setup(DEFAULT_RECEIVE_BUDGET - FRAGMENT_LEN + 1).assert_bounds();
    }
}
