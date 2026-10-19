use bevy_app::App;
use bevy_ecs::world::{EntityWorldMut, World};
use campfire_net_connection::{ConnectionPlugins, RawClientPlugin, ServerConnectionPlugins};
use campfire_net_core::TickRate;
use campfire_net_endpoint::EndpointPlugin;
use campfire_net_link::{LinkPlugin, ServerLinkPlugin};
use campfire_net_packets::EmulatedWires;
use campfire_net_timeline::{ClockPlugin, TimelinePlugins};

use crate::stack::Stack;

/// The connection layer alone: raw clients and a raw server, connected once linked.
///
/// Every stack adds a layer only if it is not added yet, so a server app that holds a host client
/// adds the layers both sides share once.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConnectionStack;

impl ConnectionStack {
    fn add_shared(app: &mut App, tick_rate: TickRate) {
        if !app.is_plugin_added::<ClockPlugin>() {
            app.add_plugins((
                TimelinePlugins { tick_rate },
                LinkPlugin,
                EndpointPlugin::<EmulatedWires>::default(),
                ConnectionPlugins,
            ));
        }
    }
}

impl Stack for ConnectionStack {
    fn add_client(&self, app: &mut App, tick_rate: TickRate) {
        Self::add_shared(app, tick_rate);
        if !app.is_plugin_added::<RawClientPlugin>() {
            app.add_plugins(RawClientPlugin);
        }
    }

    fn add_server(&self, app: &mut App, tick_rate: TickRate) {
        Self::add_shared(app, tick_rate);
        if !app.is_plugin_added::<ServerLinkPlugin>() {
            app.add_plugins((ServerLinkPlugin, ServerConnectionPlugins));
        }
    }

    fn add_protocol(&self, _: &mut App) {}

    fn client_link(&self, _: &mut EntityWorldMut<'_>) {}

    fn server_link(&self, _: &mut EntityWorldMut<'_>) {}

    fn is_ready(&self, _: &World) -> bool {
        true
    }
}

#[cfg(test)]
mod tests;
