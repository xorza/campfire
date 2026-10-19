//! A link's channels on its entity: the [`Transport`] component over `campfire_net_channels`, the
//! channel registry an app fills, the systems that run each link's receive and send steps in the
//! frame, and the iteration over links that the layers above share.

#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod adaptive_query_iter_mut;
mod app_channel_ext;
#[cfg(feature = "internals")]
pub mod bench;
mod channel_registration;
mod channel_registry;
#[cfg(feature = "internals")]
pub mod internals;
mod packet_received;
mod transport;
mod transport_plugin;
mod transport_systems;

pub use crate::adaptive_query_iter_mut::AdaptiveQueryIterMut;
pub use crate::app_channel_ext::AppChannelExt;
pub use crate::channel_registration::ChannelRegistration;
pub use crate::channel_registry::ChannelRegistry;
pub use crate::packet_received::PacketReceived;
pub use crate::transport::Transport;
pub use crate::transport_plugin::TransportPlugin;
pub use crate::transport_systems::TransportSystems;
