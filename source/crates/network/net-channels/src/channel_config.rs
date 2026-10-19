use crate::channel_mode::ChannelMode;
use campfire_net_core::RegistryHasher;
use core::time::Duration;

/// [`ChannelConfig`] are used to specify how the `Channel` behaves (reliability, ordering,
/// direction)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelConfig {
    /// The ordering and reliability guarantees of the channel.
    pub mode: ChannelMode,
    /// How often should we try to send messages on this channel.
    /// Set to `Duration::default()` to send messages every frame if possible.
    pub send_frequency: Duration,
    /// The channel's weight when the session's budget cannot send every message: the messages go
    /// from the highest message priority times channel priority down.
    pub priority: f32,
    /// Whether unreliable messages that the session's budget left unsent should remain queued for
    /// a later send flush.
    ///
    /// This only controls what the budget holds back. Once a message goes to the session, its
    /// retransmission is determined by [`ChannelMode`]. Reliable channels always retain
    /// locally unsent messages regardless of this setting. If any fragment of an unreliable
    /// message has already been admitted, its remaining fragments are also retained to avoid
    /// guaranteeing an incomplete local send.
    pub retry_unsent_messages: bool,
    /// The largest message this channel carries, in uncompressed bytes.
    ///
    /// A larger send is refused with [`TransportError::MessageTooLarge`]; a peer whose fragments
    /// claim a larger message breaks the protocol. It bounds what one partial message makes the
    /// receiver allocate.
    ///
    /// [`TransportError::MessageTooLarge`]: crate::transport_error::TransportError::MessageTooLarge
    pub max_message_size: usize,
}

/// The default [`ChannelConfig::max_message_size`], 512 KiB, which GameNetworkingSockets'
/// `RecvMaxMessageSize` also uses.
pub(crate) const DEFAULT_MAX_MESSAGE_SIZE: usize = 512 * 1024;

impl ChannelConfig {
    /// Adds these config to a registry's hash, so peers whose channel config differ fail
    /// the protocol check instead of every packet after it.
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
        hasher.write(&u64::try_from(self.max_message_size).unwrap().to_le_bytes());
    }
}

fn duration_bytes(duration: Duration) -> [u8; 12] {
    let mut bytes = [0; 12];
    bytes[..8].copy_from_slice(&duration.as_secs().to_le_bytes());
    bytes[8..].copy_from_slice(&duration.subsec_nanos().to_le_bytes());
    bytes
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            mode: ChannelMode::UnorderedUnreliable,
            send_frequency: Duration::default(),
            priority: 1.0,
            retry_unsent_messages: true,
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        }
    }
}
