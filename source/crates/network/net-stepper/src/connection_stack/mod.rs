use bevy_app::App;
use bevy_ecs::world::{EntityWorldMut, World};
use campfire_net_connection::{ConnectionPlugin, HostPlugin, RawClientPlugin, RawServerPlugin};
use campfire_net_core::TickRate;
use campfire_net_link::ServerLinkPlugin;
use campfire_net_timeline::TimelinePlugin;

use crate::stack::Stack;

/// The connection layer alone: raw clients and a raw server, connected once linked.
///
/// Every stack adds a plugin only if it is not added yet, so a stack built on two others that
/// share a layer adds that layer once.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConnectionStack;

impl ConnectionStack {
    fn add_shared(app: &mut App, tick_rate: TickRate) {
        if !app.is_plugin_added::<TimelinePlugin>() {
            app.add_plugins((TimelinePlugin { tick_rate }, ConnectionPlugin));
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
        if !app.is_plugin_added::<RawServerPlugin>() {
            app.add_plugins((ServerLinkPlugin, HostPlugin, RawServerPlugin));
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
