use crate::hello_plugin::HelloPlugin;
use crate::pre_spawned_plugin::PreSpawnedPlugin;
use crate::replicon_channel_registration_plugin::RepliconChannelRegistrationPlugin;
use crate::shared_component_registration_plugin::SharedComponentRegistrationPlugin;
use bevy_app::PluginGroupBuilder;
use bevy_app::prelude::PluginGroup;
use bevy_replicon::shared::{AuthMethod, RepliconSharedPlugin};

#[derive(Debug)]
pub struct ReplicationPlugins;

impl PluginGroup for ReplicationPlugins {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();

        group = group.add(RepliconSharedPlugin {
            auth_method: AuthMethod::None,
        });
        group = group.add(RepliconChannelRegistrationPlugin);
        group = group.add(HelloPlugin);
        group = group.add(PreSpawnedPlugin);
        // Register shared marker components before server/client-specific plugins,
        // so that both sides have matching replicon component IDs.
        group = group.add(SharedComponentRegistrationPlugin);

        group
    }
}

#[cfg(test)]
mod tests {
    use crate::fixtures::replication_stack::ReplicationStack;
    use crate::replication_target::Replicate;
    use bevy_app::{App, PreUpdate};
    use bevy_ecs::entity::Entity;
    use bevy_ecs::event::EntityEvent;
    use bevy_ecs::observer::On;
    use bevy_ecs::prelude::{ResMut, Resource};
    use bevy_ecs::schedule::Schedules;
    use bevy_replicon::shared::server_entity_map::ServerEntityMap;
    use campfire_net_connection::NetworkTarget;
    use campfire_net_messages::EventSender;
    use campfire_net_messages::RemoteEvent;
    use campfire_net_messages::internals::MappedEntityEvent;
    use campfire_net_stepper::Stepper;
    use campfire_net_stepper::StepperConfig;
    use campfire_net_transport::internals::Channel1;

    /// The message receive drains every channel of a transport, replicon's too, so each app
    /// reads replicon's channels before it, as the replicon plugins order it: in `PreUpdate`,
    /// the packet read of each side comes first.
    #[test]
    fn replicon_reads_its_channels_before_the_message_receive() {
        let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());
        stepper.frame_step(1);
        let position = |app: &App, name: &str| {
            app.world()
                .resource::<Schedules>()
                .get(PreUpdate)
                .unwrap()
                .systems()
                .unwrap()
                .position(|(_, system)| system.name().to_string().ends_with(name))
                .unwrap_or_else(|| panic!("{name} runs in PreUpdate"))
        };
        for (app, read) in [
            (
                stepper.server_app(),
                "ServerRepliconPlugin::receive_server_packets",
            ),
            (
                stepper.client_app(0),
                "ClientRepliconPlugin::receive_client_packets",
            ),
        ] {
            assert!(position(app, read) < position(app, "MessagePlugin::receive"));
        }
    }

    #[derive(Resource, Default)]
    struct Targets(Vec<Entity>);

    fn collect(
        trigger: On<'_, '_, RemoteEvent<MappedEntityEvent>>,
        mut targets: ResMut<'_, Targets>,
    ) {
        targets.0.push(trigger.event.event_target());
    }

    /// An event that names a replicated entity or a link entity arrives naming the receiver's
    /// own, each way: the client maps through replicon's map and the pairs of the hello, and the
    /// server keeps its entities.
    #[test]
    fn an_event_names_the_receivers_entities_each_way() {
        let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());
        let server_entity = stepper
            .server_app_mut()
            .world_mut()
            .spawn(Replicate::to_clients(NetworkTarget::All))
            .id();
        stepper.frame_step(2);
        let client_entity = *stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_entity)
            .expect("the entity is replicated");
        let client = stepper.client_entity(0);
        let client_of = stepper.client_of_entity(0);
        stepper
            .client_app_mut(0)
            .init_resource::<Targets>()
            .add_observer(collect);
        stepper
            .server_app_mut()
            .init_resource::<Targets>()
            .add_observer(collect);

        let mut sender = stepper.client_of_mut(0);
        let mut sender = sender
            .insert(EventSender::<MappedEntityEvent>::default())
            .get_mut::<EventSender<MappedEntityEvent>>()
            .unwrap();
        sender.trigger::<Channel1>(MappedEntityEvent(server_entity));
        sender.trigger::<Channel1>(MappedEntityEvent(client_of));
        let mut sender = stepper.client_mut(0);
        let mut sender = sender
            .insert(EventSender::<MappedEntityEvent>::default())
            .get_mut::<EventSender<MappedEntityEvent>>()
            .unwrap();
        sender.trigger::<Channel1>(MappedEntityEvent(client_entity));
        sender.trigger::<Channel1>(MappedEntityEvent(client));
        stepper.frame_step(2);

        let targets = |app: &App| {
            let mut targets = app.world().resource::<Targets>().0.clone();
            targets.sort();
            targets
        };
        let sorted = |mut entities: [Entity; 2]| {
            entities.sort();
            entities
        };
        assert_eq!(
            targets(stepper.client_app(0)),
            sorted([client_entity, client])
        );
        assert_eq!(
            targets(stepper.server_app()),
            sorted([server_entity, client_of])
        );
    }
}
