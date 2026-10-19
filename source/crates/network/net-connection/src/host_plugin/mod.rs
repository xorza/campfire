use crate::client::Client;
use crate::client_of::ClientOf;
use crate::connect::Connect;
use crate::connection_state::Connected;
use crate::connection_state::Connecting;
use crate::connection_state::Disconnected;
use crate::connection_state::DisconnectedReason;
use crate::disconnect::Disconnect;
use crate::host_client::HostClient;
use crate::local_id::LocalId;
use crate::remote_id::RemoteId;
use crate::server_state::Started;
use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use campfire_net_core::PeerId;
use campfire_net_link::{LinkOf, Server};
use tracing::info;

#[derive(Debug)]
pub struct HostPlugin;

impl HostPlugin {
    /// A host-server client gets connected automatically to the server.
    ///
    /// Starting the server link can be asynchronous, so the client may request a connection before
    /// the server is [`Started`]. The client remains [`Connecting`] until the server starts.
    fn connect(
        trigger: On<'_, '_, Connect>,
        mut commands: Commands<'_, '_>,
        query: Query<'_, '_, &LinkOf, (With<Client>, Without<HostClient>)>,
        server_query: Query<'_, '_, Has<Started>, With<Server>>,
    ) {
        let Ok(link_of) = query.get(trigger.entity) else {
            return;
        };
        let Ok(server_started) = server_query.get(link_of.server) else {
            return;
        };
        if !server_started {
            commands.entity(trigger.entity).insert(Connecting);
            return;
        }

        info!(entity=?trigger.entity, "Connected host-client");
        commands.entity(trigger.entity).insert((
            Connected,
            // We cannot insert the ids purely from the point of view of the client
            // so we set both its to Local
            LocalId(PeerId::Local(0)),
            RemoteId(PeerId::Local(0)),
            ClientOf,
            // NOTE: it's very important to insert Connected and HostClient at the same time
            //  to avoid race conditions between observers that depend on Connected, and those
            // that depend on HostClient
            HostClient { buffer: Vec::new() },
        ));
    }

    fn disconnect(
        trigger: On<'_, '_, Disconnect>,
        mut commands: Commands<'_, '_>,
        query: Query<'_, '_, &LinkOf, (With<Client>, Or<(With<HostClient>, With<Connecting>)>)>,
    ) {
        if query.contains(trigger.entity) {
            info!(entity=?trigger.entity,"Disconnected host-client");
            commands.entity(trigger.entity).try_insert(Disconnected {
                reason: DisconnectedReason::UserRequested(trigger.reason),
            });
        }
    }

    /// A host client is a host client only while connected, whatever disconnected it.
    fn end_host_on_disconnect(
        trigger: On<'_, '_, Add<Disconnected>>,
        query: Query<'_, '_, (), With<HostClient>>,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            commands.entity(trigger.entity).try_remove::<HostClient>();
        }
    }

    fn check_if_host_on_client_change(
        // NOTE: we handle Connecting in the trigger because otherwise the client
        //  would never be Connected
        trigger: On<'_, '_, Add<(Client, Connected, LinkOf)>>,
        client_query: Query<'_, '_, &LinkOf, (With<Client>, With<Connected>, Without<HostClient>)>,
        server_query: Query<'_, '_, (), (With<Started>, With<Server>)>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok(link_of) = client_query.get(trigger.entity)
            && server_query.get(link_of.server).is_ok()
        {
            commands
                .entity(trigger.entity)
                .insert(HostClient { buffer: Vec::new() });
        }
    }

    fn check_if_host_on_server_change(
        trigger: On<'_, '_, Add<(Server, Started)>>,
        server_query: Query<'_, '_, &Server, With<Started>>,
        client_query: Query<
            '_,
            '_,
            (Has<Connected>, Has<Connecting>),
            (With<Client>, Without<HostClient>),
        >,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok(server) = server_query.get(trigger.entity) {
            for client in server.collection() {
                if let Ok((connected, connecting)) = client_query.get(*client) {
                    if connecting {
                        // The client may have requested a connection before the server link was
                        // ready. `connect` leaves that request pending until `Started` is added, so
                        // retry it now that the server link is ready.
                        commands.trigger(Connect { entity: *client });
                    } else if connected {
                        commands
                            .entity(*client)
                            .insert(HostClient { buffer: Vec::new() });
                    }
                }
            }
        }
    }
}

impl Plugin for HostPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(Self::connect);
        app.add_observer(Self::disconnect);
        app.add_observer(Self::end_host_on_disconnect);
        app.add_observer(Self::check_if_host_on_client_change);
        app.add_observer(Self::check_if_host_on_server_change);
    }
}

#[cfg(test)]
mod tests;
