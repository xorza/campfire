use crate::client::Client;
use crate::connection_state::Connected;
use campfire_common::ErrorReport;

use crate::connection_state::Disconnected;
use crate::network_topology::NetworkTopology;
use crate::network_topology_error::NetworkTopologyError;
use crate::network_topology_systems::NetworkTopologySystems;
use crate::networking_state::NetworkingState;
use crate::server_state::Started;
use crate::server_state::Stopped;
use bevy_app::{App, Plugin, PostUpdate};
use bevy_ecs::prelude::*;
use campfire_net_link::{LinkSystems, Server};
use smallvec::SmallVec;

type TopologyComponents = (Client, Server, Connected, Disconnected, Started, Stopped);

#[derive(Debug)]
pub(crate) struct NetworkTopologyPlugin;

impl Plugin for NetworkTopologyPlugin {
    fn build(&self, app: &mut App) {
        // NetworkingState starts dirty so that entities spawned before this plugin are
        // classified on the first update too.
        app.init_resource::<NetworkingState>();

        app.add_observer(Self::mark_dirty_on_insert);
        app.add_observer(Self::mark_dirty_on_remove);

        // Reconcile lifecycle changes once per frame.
        app.configure_sets(
            PostUpdate,
            NetworkTopologySystems::Update.before(LinkSystems::Send),
        );
        app.add_systems(
            PostUpdate,
            Self::refresh_network_topology
                .in_set(NetworkTopologySystems::Update)
                .run_if(Self::network_topology_is_dirty),
        );
    }
}

fn unique_ready_client(
    mut clients: impl Iterator<Item = Entity>,
) -> Result<Option<Entity>, NetworkTopologyError> {
    let Some(first) = clients.next() else {
        return Ok(None);
    };
    let Some(second) = clients.next() else {
        return Ok(Some(first));
    };

    let mut entities: SmallVec<[Entity; 4]> = SmallVec::from_slice(&[first, second]);
    entities.extend(clients);
    entities.sort_unstable_by_key(|entity| entity.index_u32());
    Err(NetworkTopologyError::MultipleConnectedClients(entities))
}

fn unique_ready_server(
    mut servers: impl Iterator<Item = Entity>,
) -> Result<Option<Entity>, NetworkTopologyError> {
    let Some(first) = servers.next() else {
        return Ok(None);
    };
    let Some(second) = servers.next() else {
        return Ok(Some(first));
    };

    let mut entities: SmallVec<[Entity; 4]> = SmallVec::from_slice(&[first, second]);
    entities.extend(servers);
    entities.sort_unstable_by_key(|entity| entity.index_u32());
    Err(NetworkTopologyError::MultipleStartedServers(entities))
}

const fn infer_standard_topology(
    client: Option<Entity>,
    server: Option<Entity>,
) -> NetworkTopology {
    match (client, server) {
        (None, None) => NetworkTopology::Undefined,
        (None, Some(server)) => NetworkTopology::Server(server),
        (Some(client), None) => NetworkTopology::Client(client),
        (Some(client), Some(server)) => {
            NetworkTopology::Invalid(NetworkTopologyError::MixedClientServer { client, server })
        }
    }
}

impl NetworkTopologyPlugin {
    fn mark_dirty_on_insert(
        _trigger: On<'_, '_, Insert<TopologyComponents>>,
        mut networking: ResMut<'_, NetworkingState>,
    ) {
        networking.bypass_change_detection().dirty = true;
    }

    fn mark_dirty_on_remove(
        _trigger: On<'_, '_, Remove<TopologyComponents>>,
        mut networking: ResMut<'_, NetworkingState>,
    ) {
        networking.bypass_change_detection().dirty = true;
    }

    fn network_topology_is_dirty(networking: Res<'_, NetworkingState>) -> bool {
        networking.dirty
    }

    fn refresh_network_topology(
        mut networking: ResMut<'_, NetworkingState>,
        ready_clients: Query<'_, '_, Entity, (With<Client>, With<Connected>)>,
        ready_servers: Query<'_, '_, Entity, (With<Server>, With<Started>)>,
    ) {
        let next = match unique_ready_client(ready_clients.iter()) {
            Err(error) => NetworkTopology::Invalid(error),
            Ok(client) => match unique_ready_server(ready_servers.iter()) {
                Err(error) => NetworkTopology::Invalid(error),
                Ok(server) => infer_standard_topology(client, server),
            },
        };

        let topology_changed = networking.topology != next;
        networking.bypass_change_detection().dirty = false;
        if topology_changed {
            if let NetworkTopology::Invalid(error) = &next {
                tracing::error!(error = %ErrorReport::of(error), "invalid networking topology");
            }
            networking.topology = next;
        }
    }
}

#[cfg(test)]
mod tests;
