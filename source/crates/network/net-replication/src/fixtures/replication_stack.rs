use bevy_app::App;
use bevy_ecs::world::{EntityWorldMut, World};
use bevy_state::app::StatesPlugin;
use campfire_net_core::TickRate;
use campfire_net_messages::internals::MessagesStack;
use campfire_net_stepper::Stack;

use crate::client_replication_plugin::ClientReplicationPlugin;
use crate::client_replication_plugins::ClientReplicationPlugins;
use crate::fixtures::protocol::ReplicationProtocolPlugin;
use crate::hello_plugin::HelloPlugin;
use crate::replication_plugins::ReplicationPlugins;
use crate::replication_receiver::ReplicationReceiver;
use crate::replication_sender::ReplicationSender;
use crate::room_plugin::RoomPlugin;
use crate::server_replication_plugins::ServerReplicationPlugins;
use crate::server_send_plugin::ServerSendPlugin;

/// The messages layer with replication both ways on each link, and the test components.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReplicationStack;

impl ReplicationStack {
    fn add_shared(app: &mut App) {
        if !app.is_plugin_added::<StatesPlugin>() {
            app.add_plugins(StatesPlugin);
        }
        if !app.is_plugin_added::<HelloPlugin>() {
            app.add_plugins(ReplicationPlugins);
        }
    }
}

impl Stack for ReplicationStack {
    fn add_client(&self, app: &mut App, tick_rate: TickRate) {
        MessagesStack.add_client(app, tick_rate);
        Self::add_shared(app);
        if !app.is_plugin_added::<ClientReplicationPlugin>() {
            app.add_plugins(ClientReplicationPlugins);
        }
    }

    fn add_server(&self, app: &mut App, tick_rate: TickRate) {
        MessagesStack.add_server(app, tick_rate);
        Self::add_shared(app);
        if !app.is_plugin_added::<ServerSendPlugin>() {
            app.add_plugins((ServerReplicationPlugins, RoomPlugin));
        }
    }

    fn add_protocol(&self, app: &mut App) {
        MessagesStack.add_protocol(app);
        if !app.is_plugin_added::<ReplicationProtocolPlugin>() {
            app.add_plugins(ReplicationProtocolPlugin);
        }
    }

    fn client_link(&self, link: &mut EntityWorldMut<'_>) {
        MessagesStack.client_link(link);
        link.insert((ReplicationSender, ReplicationReceiver));
    }

    fn server_link(&self, link: &mut EntityWorldMut<'_>) {
        MessagesStack.server_link(link);
        link.insert((ReplicationSender, ReplicationReceiver));
    }

    fn is_ready(&self, world: &World) -> bool {
        MessagesStack.is_ready(world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::protocol::CompA;
    use crate::replication_target::Replicate;
    use bevy_ecs::query::With;
    use bevy_replicon::prelude::Remote;
    use campfire_net_connection::NetworkTarget;
    use campfire_net_stepper::Stepper;
    use campfire_net_stepper::StepperSetup;

    /// A replicated entity reaches the client with its component.
    #[test]
    fn an_entity_replicates_to_the_client() {
        let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
        stepper
            .server_app_mut()
            .world_mut()
            .spawn((CompA(1.0), Replicate::to_clients(NetworkTarget::All)));
        stepper.frame_step(4);
        let world = stepper.client_app_mut(0).world_mut();
        let replicated = world
            .query_filtered::<&CompA, With<Remote>>()
            .iter(world)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(replicated, [CompA(1.0)]);
    }
}
