use crate::client::Client;
use crate::connect::Connect;
use crate::connection_state::Connected;
use crate::connection_state::Connecting;
use crate::connection_state::Disconnected;
use crate::disconnect::Disconnect;
use crate::local_id::LocalId;
use crate::networking_state::NetworkingState;
use crate::remote_id::RemoteId;
use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use campfire_net_core::PeerId;
use campfire_net_link::LocalAddress;
use campfire_net_link::{LinkOf, LinkStart, Linked, Linking, Unlink};
use campfire_net_link::{Server, Unlinked};
use campfire_net_payload::UnlinkReason;
use tracing::trace;

/// The lifecycle of each client: a client's link is its connection, so each state of the link
/// is the connection's, except on a host client, which has no link of its own.
#[derive(Debug)]
pub(crate) struct ClientConnectionPlugin;

impl ClientConnectionPlugin {
    /// When the client request to connect, we also try to establish the link
    fn connect(connect: On<'_, '_, Connect>, mut commands: Commands<'_, '_>) {
        trace!("Triggering LinkStart because Connect was triggered");
        commands.trigger(LinkStart {
            entity: connect.entity,
        });
    }

    /// A connection whose link unlinks is disconnected, for the reason its link gives.
    fn disconnect_if_link_fails(
        trigger: On<'_, '_, Add<Unlinked>>,
        query: Query<'_, '_, &Unlinked, (Without<Disconnected>, Without<Server>)>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok(unlinked) = query.get(trigger.entity) {
            trace!(
                entity = ?trigger.entity,
                "Adding Disconnected because the link got Unlinked (reason: {:?})",
                unlinked.reason
            );
            commands.entity(trigger.entity).try_insert(Disconnected {
                reason: unlinked.reason.into(),
            });
        }
    }

    fn connect_on_linking(
        trigger: On<'_, '_, Add<Linking>>,
        query: Query<'_, '_, (), (With<Client>, Without<LinkOf>)>,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            commands.entity(trigger.entity).try_insert(Connecting);
        }
    }

    /// The ids the IO layer set are kept; else the local id is the link's address and the remote
    /// one the server.
    fn connected_on_linked(
        trigger: On<'_, '_, Add<Linked>>,
        query: Query<
            '_,
            '_,
            (&LocalAddress, Option<&LocalId>, Option<&RemoteId>),
            (With<Client>, Without<LinkOf>),
        >,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok((local_address, local_id, remote_id)) = query.get(trigger.entity) {
            trace!("Client Linked! Adding Connected");
            commands.entity(trigger.entity).try_insert((
                Connected,
                local_id
                    .copied()
                    .unwrap_or(LocalId(PeerId::Address(local_address.0))),
                remote_id.copied().unwrap_or(RemoteId(PeerId::Server)),
            ));
        }
    }

    /// A `Disconnect` unlinks, which reports the disconnect's code.
    fn unlink_on_disconnect(
        trigger: On<'_, '_, Disconnect>,
        query: Query<'_, '_, (), (With<Client>, Without<LinkOf>, Without<Disconnected>)>,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            trace!("Client Disconnect! Triggering Unlink");
            commands.trigger(Unlink {
                entity: trigger.entity,
                reason: UnlinkReason::Closed(trigger.reason),
            });
        }
    }
}

impl Plugin for ClientConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NetworkingState>();
        app.add_observer(Self::connect);
        app.add_observer(Self::disconnect_if_link_fails);
        app.add_observer(Self::connect_on_linking);
        app.add_observer(Self::connected_on_linked);
        app.add_observer(Self::unlink_on_disconnect);
    }
}

#[cfg(test)]
mod tests;
