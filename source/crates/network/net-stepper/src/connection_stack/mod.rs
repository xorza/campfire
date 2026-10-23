use bevy_app::App;
use bevy_ecs::world::{EntityWorldMut, World};
use bevy_time::TimePlugin;
use campfire_app_threads::OneThreadPlugins;
use campfire_math::TickRate;
use campfire_net_connection::{ConnectionPlugins, ServerConnectionPlugin};
use campfire_net_endpoint::EndpointPlugin;
use campfire_net_link::{LinkPlugin, ServerLinkPlugin};
use campfire_net_packets::EmulatedWires;
use campfire_net_timeline::{ClockPlugin, TimelinePlugins};

use crate::stack::Stack;

/// The connection layer alone: clients and a server, connected once linked.
///
/// Every stack adds a layer only if it is not added yet, so a stack that builds on two layers,
/// which both build on this one, adds it once.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConnectionStack;

impl ConnectionStack {
    fn add_shared(app: &mut App, tick_rate: TickRate) {
        if !app.is_plugin_added::<ClockPlugin>() {
            app.add_plugins((
                OneThreadPlugins,
                TimePlugin,
                TimelinePlugins { tick_rate },
                LinkPlugin,
                EndpointPlugin::<EmulatedWires>::default(),
                ConnectionPlugins,
            ));
        }
    }
}

impl Stack for ConnectionStack {
    fn add_client(&self, app: &mut App, _: usize, tick_rate: TickRate) {
        Self::add_shared(app, tick_rate);
    }

    fn add_server(&self, app: &mut App, tick_rate: TickRate) {
        Self::add_shared(app, tick_rate);
        if !app.is_plugin_added::<ServerLinkPlugin>() {
            app.add_plugins((ServerLinkPlugin, ServerConnectionPlugin));
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
