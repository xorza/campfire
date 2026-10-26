use campfire_net_channels::{Channel, ChannelMode, ChannelSetup};
use campfire_net_core::NetId;

/// The channel the protocol check travels on, the first every channel registry holds, so its id
/// is the same in every build, whatever else the app registers: the check is the one message a
/// peer of another build still reads (11-net-stack/transport.md, The protocol check goes first on
/// a reserved channel).
#[derive(Debug)]
pub(crate) struct ProtocolChannel;

impl Channel for ProtocolChannel {}

impl ProtocolChannel {
    /// Its id, the registry's first.
    pub(crate) const ID: NetId = NetId::new(0);

    /// Its setup: reliable, and first among a flush's channels, with room for every registry's
    /// hash.
    pub(crate) fn setup() -> ChannelSetup {
        ChannelSetup {
            mode: ChannelMode::OrderedReliable,
            priority: 100.0,
            max_message_len: 1024,
            ..ChannelSetup::default()
        }
    }
}
