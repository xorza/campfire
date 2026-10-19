use crate::despawn;
use crate::interpolated_despawn::InterpolatedDespawn;
use crate::pre_spawned_plugin::PreSpawnedPlugin;
use crate::pre_spawned_systems::PreSpawnedSystems;
use crate::remote_parent::RemoteParent;
use bevy_app::prelude::{App, Plugin};
use bevy_app::prelude::{PostUpdate, PreUpdate};
use bevy_ecs::prelude::{IntoScheduleConfigs, resource_exists_and_changed};
use bevy_replicon::client::ClientSystems;
use bevy_replicon::shared::replication::registry::ReplicationRegistry;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;

#[derive(Debug)]
pub(crate) struct ClientReplicationPlugin;

impl Plugin for ClientReplicationPlugin {
    fn build(&self, app: &mut App) {
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
            RemoteParent::attach
                .after(ClientSystems::Receive)
                .run_if(resource_exists_and_changed::<ServerEntityMap>),
        );
    }
}
