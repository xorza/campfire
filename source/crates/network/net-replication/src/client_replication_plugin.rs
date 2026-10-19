use crate::client_replicon_plugin::ClientRepliconPlugin;
use crate::despawn;
use crate::hierarchy;
use crate::interpolated_despawn::InterpolatedDespawn;
use crate::pre_spawned_plugin::PreSpawnedPlugin;
use crate::pre_spawned_systems::PreSpawnedSystems;
use bevy_app::prelude::{App, Plugin};
use bevy_app::prelude::{PostUpdate, PreUpdate};
use bevy_ecs::prelude::{IntoScheduleConfigs, resource_exists_and_changed};
use bevy_replicon::client::{ClientPlugin, ClientSystems};
use bevy_replicon::shared::replication::registry::ReplicationRegistry;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;

#[derive(Debug)]
pub struct ClientReplicationPlugin;

impl Plugin for ClientReplicationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ClientPlugin);
        app.add_plugins(ClientRepliconPlugin);
        app.init_resource::<InterpolatedDespawn>();
        app.init_resource::<ReplicationRegistry>();
        app.world_mut()
            .resource_mut::<ReplicationRegistry>()
            .despawn = despawn::receive;
        app.add_systems(
            PostUpdate,
            PreSpawnedPlugin::expire_unmatched_pre_spawns.in_set(PreSpawnedSystems::CleanUp),
        );
        // A detached parent can only resolve once the receive mapped a new entity.
        app.add_systems(
            PreUpdate,
            hierarchy::attach_remote_parents
                .after(ClientSystems::Receive)
                .run_if(resource_exists_and_changed::<ServerEntityMap>),
        );
    }
}
