use bevy_app::App;
use bevy_ecs::world::{EntityWorldMut, World};
use campfire_net_core::TickRate;
use campfire_net_stepper::Stack;
use campfire_net_transport::internals::TransportStack;

use crate::fixtures::protocol::MessagesProtocolPlugin;
use crate::message_manager::MessageManager;
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
    fn add_client(&self, app: &mut App, tick_rate: TickRate) {
        TransportStack.add_client(app, tick_rate);
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
        link.insert_if_new(MessageManager::default());
    }

    fn server_link(&self, link: &mut EntityWorldMut<'_>) {
        TransportStack.server_link(link);
        link.insert_if_new(MessageManager::default());
    }

    fn is_ready(&self, world: &World) -> bool {
        TransportStack.is_ready(world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::protocol::StringMessage;
    use crate::message_receiver::MessageReceiver;
    use crate::message_sender::MessageSender;
    use bevy_app::Update;
    use bevy_ecs::prelude::{Query, ResMut, Resource};
    use campfire_net_stepper::Stepper;
    use campfire_net_stepper::StepperConfig;
    use campfire_net_transport::internals::Channel1;

    #[derive(Resource, Default)]
    struct Received(Vec<StringMessage>);

    /// The client clears unread messages at the end of each frame, so they are read in it.
    fn collect(
        mut receivers: Query<'_, '_, &mut MessageReceiver<StringMessage>>,
        mut received: ResMut<'_, Received>,
    ) {
        for mut receiver in &mut receivers {
            received.0.extend(receiver.receive());
        }
    }

    /// A message the server sends reaches the client, and a host client connects in the server's
    /// app.
    #[test]
    fn a_message_reaches_the_client() {
        let mut stepper = Stepper::new(MessagesStack, StepperConfig::host_server());
        stepper
            .client_app_mut(0)
            .init_resource::<Received>()
            .add_systems(Update, collect);
        stepper
            .client_of_mut(0)
            .insert(MessageSender::<StringMessage>::default())
            .get_mut::<MessageSender<StringMessage>>()
            .unwrap()
            .send::<Channel1>(StringMessage("hello".to_string()));
        stepper.frame_step(2);
        let received = &stepper.client_app(0).world().resource::<Received>().0;
        assert_eq!(received, &[StringMessage("hello".to_string())]);
    }
}
