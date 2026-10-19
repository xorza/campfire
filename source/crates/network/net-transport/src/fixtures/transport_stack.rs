use bevy_app::App;
use bevy_ecs::world::{EntityWorldMut, World};
use campfire_math::TickRate;
use campfire_net_stepper::ConnectionStack;
use campfire_net_stepper::Stack;

use crate::fixtures::protocol::TransportProtocolPlugin;
use crate::transport::Transport;
use crate::transport_plugin::TransportPlugin;

/// The connection layer with a transport on each link, and the test channels.
#[derive(Debug, Clone, Copy, Default)]
pub struct TransportStack;

impl TransportStack {
    fn add_layer(app: &mut App) {
        if !app.is_plugin_added::<TransportPlugin>() {
            app.add_plugins(TransportPlugin);
        }
    }
}

impl Stack for TransportStack {
    fn add_client(&self, app: &mut App, tick_rate: TickRate) {
        ConnectionStack.add_client(app, tick_rate);
        Self::add_layer(app);
    }

    fn add_server(&self, app: &mut App, tick_rate: TickRate) {
        ConnectionStack.add_server(app, tick_rate);
        Self::add_layer(app);
    }

    fn add_protocol(&self, app: &mut App) {
        ConnectionStack.add_protocol(app);
        if !app.is_plugin_added::<TransportProtocolPlugin>() {
            app.add_plugins(TransportProtocolPlugin);
        }
    }

    fn client_link(&self, link: &mut EntityWorldMut<'_>) {
        ConnectionStack.client_link(link);
        link.insert_if_new(Transport::default());
    }

    fn server_link(&self, link: &mut EntityWorldMut<'_>) {
        ConnectionStack.server_link(link);
        link.insert_if_new(Transport::default());
    }

    fn is_ready(&self, world: &World) -> bool {
        ConnectionStack.is_ready(world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::protocol::{Channel1, channel_id};
    use bytes::Bytes;
    use campfire_net_channels::ChannelReceiver;
    use campfire_net_stepper::Stepper;
    use campfire_net_stepper::StepperSetup;

    /// A raw message the server sends on the stack's channel reaches the client's transport.
    #[test]
    fn a_raw_message_reaches_the_client() {
        let mut stepper = Stepper::new(TransportStack, StepperSetup::single());
        let channel_id = channel_id::<Channel1>(stepper.server_app());
        stepper
            .client_of_mut(0)
            .get_mut::<Transport>()
            .unwrap()
            .send(channel_id, Bytes::from_static(b"hello"), 1.0)
            .unwrap();
        stepper.frame_step(2);
        let mut client = stepper.client_mut(0);
        let mut transport = client.get_mut::<Transport>().unwrap();
        let received = transport
            .channel_receivers_mut()
            .find_map(ChannelReceiver::read_message)
            .map(|message| message.bytes);
        assert_eq!(received, Some(Bytes::from_static(b"hello")));
    }
}
