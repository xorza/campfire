//! A link's channels on its entity: the [`Transport`] component over `campfire_net_channels`, the
//! channel registry an app fills, the systems that run each link's receive and send steps in the
//! frame, and the iteration over links that the layers above share.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod app_channel_ext;
#[cfg(feature = "internals")]
pub mod bench;
mod channel_reader;
mod channel_registration;
mod channel_registry;
#[cfg(feature = "internals")]
mod fixtures;
mod packet_received;
mod transport;
mod transport_plugin;
mod transport_systems;

pub use crate::app_channel_ext::AppChannelExt;
pub use crate::channel_reader::ChannelReader;
pub use crate::channel_registration::ChannelRegistration;
pub use crate::channel_registry::ChannelRegistry;
pub use crate::packet_received::PacketReceived;
pub use crate::transport::Transport;
pub use crate::transport_plugin::TransportPlugin;
pub use crate::transport_systems::TransportSystems;

/// What the tests and benches of the layers above share: this layer's stack, test protocol,
/// fixtures and measurements.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::fixtures::compression_case::{CompressionCase, CompressionRun};
    pub use crate::fixtures::protocol::{
        Channel1, ChannelOrderedReliable, ChannelSequencedReliable, ChannelSequencedUnreliable,
        ChannelUnorderedReliable, TransportProtocolPlugin, channel_id,
    };
    pub use crate::fixtures::stepper_counts::summed_transport_counts;
    pub use crate::fixtures::test_transport_plugin::{TestChannel, TestTransportPlugin};
    pub use crate::fixtures::transport_stack::TransportStack;
}
