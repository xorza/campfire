//! The layers each app kind's plugin group lists: those both sides share, then a client's and a
//! server's own, so a host server, which is both, lists the shared ones once
//! (11-net-stack/api.md, One plugin group for each kind of app, and no plugin adds another).

use bevy_app::PluginGroupBuilder;
use campfire_math::TickRate;
use campfire_net_connection::{ConnectionPlugins, ServerConnectionPlugin};
#[cfg(any(feature = "emulator", feature = "udp"))]
use campfire_net_endpoint::{EndpointPlugin, PacketSessionLinkPlugin};
use campfire_net_frame_interpolation::FrameRegistryPlugin;
use campfire_net_interpolation::{
    InterpolationMarkerPlugin, InterpolationPlugins, InterpolationRegistryPlugin,
};
use campfire_net_link::{LinkPlugin, ServerLinkPlugin};
use campfire_net_messages::MessagePlugin;
#[cfg(feature = "emulator")]
use campfire_net_packets::EmulatedWires;
use campfire_net_prediction::{
    PredictionMarkerPlugin, PredictionPlugins, PredictionRegistryPlugin,
};
use campfire_net_replication::{
    ClientReplicationPlugins, ReplicationPlugins, ServerReplicationPlugins,
};
#[cfg(feature = "udp")]
use campfire_net_socket::UdpIo;
use campfire_net_sync::{ClientSyncPlugins, SyncPlugins};
use campfire_net_timeline::TimelinePlugins;
use campfire_net_transport::TransportPlugin;
use core::num::NonZeroU32;

/// The default tick rate, 60 Hz.
pub(crate) const DEFAULT_TICK_RATE: TickRate = TickRate::hertz(NonZeroU32::new(60).unwrap());

/// The layers both sides run, at `tick_rate`. The markers of prediction and interpolation are
/// part of Replicon's protocol, so each side registers them before the app's protocol.
pub(crate) fn shared(group: PluginGroupBuilder, tick_rate: TickRate) -> PluginGroupBuilder {
    let group = group
        .add_group(TimelinePlugins { tick_rate })
        .add(LinkPlugin);
    #[cfg(any(feature = "emulator", feature = "udp"))]
    let group = group.add(PacketSessionLinkPlugin);
    #[cfg(feature = "emulator")]
    let group = group.add(EndpointPlugin::<EmulatedWires>::default());
    #[cfg(feature = "udp")]
    let group = group.add(EndpointPlugin::<UdpIo>::default());
    group
        .add(TransportPlugin)
        .add(MessagePlugin)
        .add_group(ConnectionPlugins)
        .add_group(ReplicationPlugins)
        .add(PredictionMarkerPlugin)
        .add(InterpolationMarkerPlugin)
        .add(InterpolationRegistryPlugin)
        .add(FrameRegistryPlugin)
        .add(PredictionRegistryPlugin)
        .add_group(SyncPlugins)
}

/// A client's own layers, on the shared ones.
pub(crate) fn client(group: PluginGroupBuilder) -> PluginGroupBuilder {
    group
        .add_group(ClientSyncPlugins)
        .add_group(ClientReplicationPlugins)
        .add_group(PredictionPlugins)
        .add_group(InterpolationPlugins)
}

/// A server's own layers, on the shared ones.
pub(crate) fn server(group: PluginGroupBuilder) -> PluginGroupBuilder {
    group
        .add(ServerLinkPlugin)
        .add_group(ServerReplicationPlugins)
        .add(ServerConnectionPlugin)
}

#[cfg(test)]
mod tests;
