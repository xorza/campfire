//! Bevy [`Plugin`] used by both the server and the client
use bevy_app::{App, Plugin};
use bevy_ecs::prelude::Resource;
use campfire_net_connection::ConnectionPlugin;
use campfire_net_core::TickRate;
#[cfg(any(feature = "sim", feature = "udp"))]
use campfire_net_endpoint::EndpointPlugin;
use campfire_net_interpolation::InterpolationMarkerPlugin;
use campfire_net_interpolation::InterpolationRegistryPlugin;
use campfire_net_link::LinkPlugin;
use campfire_net_messages::MessagePlugin;
#[cfg(feature = "sim")]
use campfire_net_packets::EmulatedWires;
use campfire_net_prediction::PredictionMarkerPlugin;
use campfire_net_prediction::PredictionRegistryPlugin;
use campfire_net_replication::ReplicationPlugins;
use campfire_net_timeline::TimelinePlugin;
use campfire_net_transport::TransportPlugin;
#[cfg(feature = "udp")]
use campfire_net_udp::UdpIo;
use core::num::NonZeroU16;

use crate::protocol_check_plugin::ProtocolCheckPlugin;

/// The default tick rate, 60 Hz.
pub(crate) const DEFAULT_TICK_RATE: TickRate = TickRate::new(NonZeroU16::new(60).unwrap());

#[derive(Debug)]
pub(crate) struct SharedPlugin {
    pub(crate) tick_rate: TickRate,
}

/// The tick rate of the `SharedPlugin` an app built: a host server adds both plugin
/// groups, and builds the shared layers once.
#[derive(Resource, Debug)]
struct SharedPluginsBuilt {
    tick_rate: TickRate,
}

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        if let Some(built) = app.world().get_resource::<SharedPluginsBuilt>() {
            assert_eq!(
                built.tick_rate, self.tick_rate,
                "the client and server plugin groups of one app run at one tick rate"
            );
            return;
        }
        app.insert_resource(SharedPluginsBuilt {
            tick_rate: self.tick_rate,
        });
        app.add_plugins(TimelinePlugin {
            tick_rate: self.tick_rate,
        })
        .add_plugins(TransportPlugin)
        .add_plugins(MessagePlugin)
        .add_plugins(ConnectionPlugin);

        app.add_plugins(ReplicationPlugins)
            .add_plugins(ProtocolCheckPlugin);

        // Receive-marker registration is part of Replicon's protocol and must
        // be identical on clients and servers before the user's protocol runs.
        app.add_plugins(PredictionMarkerPlugin);
        app.add_plugins(InterpolationMarkerPlugin);
        app.add_plugins(InterpolationRegistryPlugin);
        app.add_plugins(PredictionRegistryPlugin);

        if !app.is_plugin_added::<LinkPlugin>() {
            app.add_plugins(LinkPlugin);
        }
        #[cfg(feature = "sim")]
        if !app.is_plugin_added::<EndpointPlugin<EmulatedWires>>() {
            app.add_plugins(EndpointPlugin::<EmulatedWires>::default());
        }
        #[cfg(feature = "udp")]
        if !app.is_plugin_added::<EndpointPlugin<UdpIo>>() {
            app.add_plugins(EndpointPlugin::<UdpIo>::default());
        }
    }

    fn is_unique(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests;
