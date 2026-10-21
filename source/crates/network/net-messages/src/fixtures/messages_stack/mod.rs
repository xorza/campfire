use bevy_app::App;
use bevy_ecs::world::{EntityWorldMut, World};
use campfire_math::TickRate;
use campfire_net_stepper::Stack;
use campfire_net_transport::internals::TransportStack;

use crate::fixtures::protocol::MessagesProtocolPlugin;
use crate::link_messages::LinkMessages;
use crate::message_plugin::MessagePlugin;

/// The transport layer with messages and events on each link, and the test messages.
#[derive(Debug, Clone, Copy, Default)]
pub struct MessagesStack;

impl MessagesStack {
    fn add_layer(app: &mut App) {
        if !app.is_plugin_added::<MessagePlugin>() {
            app.add_plugins(MessagePlugin);
        }
    }
}

impl Stack for MessagesStack {
    fn add_client(&self, app: &mut App, client: usize, tick_rate: TickRate) {
        TransportStack.add_client(app, client, tick_rate);
        Self::add_layer(app);
    }

    fn add_server(&self, app: &mut App, tick_rate: TickRate) {
        TransportStack.add_server(app, tick_rate);
        Self::add_layer(app);
    }

    fn add_protocol(&self, app: &mut App) {
        TransportStack.add_protocol(app);
        if !app.is_plugin_added::<MessagesProtocolPlugin>() {
            app.add_plugins(MessagesProtocolPlugin);
        }
    }

    fn client_link(&self, link: &mut EntityWorldMut<'_>) {
        TransportStack.client_link(link);
        link.insert_if_new(LinkMessages::default());
    }

    fn server_link(&self, link: &mut EntityWorldMut<'_>) {
        TransportStack.server_link(link);
        link.insert_if_new(LinkMessages::default());
    }

    fn is_ready(&self, world: &World) -> bool {
        TransportStack.is_ready(world)
    }
}

#[cfg(test)]
mod tests;
