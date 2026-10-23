use crate::client_of::ClientOf;
use crate::connection_state::Connected;
use crate::connection_state::Connecting;
use crate::connection_state::Disconnected;
use crate::connection_state::DisconnectedReason;
use crate::connection_state::Disconnecting;
use crate::disconnect::Disconnect;
use crate::local_id::LocalId;
use crate::remote_id::RemoteId;
use crate::server_state::Started;
use crate::server_state::Starting;
use crate::server_state::Stopped;
use crate::server_state::Stopping;
use crate::start::Start;
use crate::stop::Stop;
use bevy_app::{App, Last, Plugin};
use bevy_ecs::prelude::*;
use campfire_net_core::PeerId;
use campfire_net_link::{LinkOf, Linked, Linking, RemoteAddress, Server};
use campfire_net_link::{LinkStart, Unlink, Unlinked};
use campfire_net_payload::UnlinkReason;
use core::fmt::Debug;
use tracing::trace;

/// Starts and stops servers with their links, and ends every server-side link: a server's link is
/// its start, each of its links a connection, and a link is despawned once it is
/// [`Disconnected`], for any cause, and a reconnect is a new entity.
#[derive(Debug)]
pub struct ServerConnectionPlugin;

impl ServerConnectionPlugin {
    /// Starting a server starts its link.
    fn start(trigger: On<'_, '_, Start>, mut commands: Commands<'_, '_>) {
        trace!("Triggering LinkStart because Start was triggered");
        commands.trigger(LinkStart {
            entity: trigger.entity,
        });
    }

    /// A server whose link unlinks is stopped, for the reason its link gives.
    fn stop_if_link_fails(
        trigger: On<'_, '_, Add<Unlinked>>,
        query: Query<'_, '_, &Unlinked, (With<Server>, Without<Stopped>)>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok(unlinked) = query.get(trigger.entity) {
            trace!("Inserting Stopped because the server's link got Unlinked");
            commands.entity(trigger.entity).try_insert(Stopped {
                reason: unlinked.reason.into(),
            });
        }
    }

    /// A [`Disconnect`] on a server's link to a remote client takes it down at the end of the
    /// frame, so that the packets sent this frame leave first.
    fn disconnect(
        trigger: On<'_, '_, Disconnect>,
        query: Query<'_, '_, (), (With<ClientOf>, Without<Disconnected>)>,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            commands.entity(trigger.entity).try_insert(Disconnecting {
                reason: trigger.reason,
            });
        }
    }

    /// An observer and not a required `Stopped`: a required state joins the state a spawn carries,
    /// and each state's hook then removes the other, which leaves none.
    fn stopped_on_add(
        trigger: On<'_, '_, Add<Server>>,
        query: Query<
            '_,
            '_,
            (),
            (
                Without<Starting>,
                Without<Started>,
                Without<Stopping>,
                Without<Stopped>,
            ),
        >,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            commands
                .entity(trigger.entity)
                .try_insert(Stopped::default());
        }
    }

    fn starting_on_linking(
        trigger: On<'_, '_, Add<Linking>>,
        query: Query<'_, '_, (), With<Server>>,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            commands.entity(trigger.entity).try_insert(Starting);
        }
    }

    fn started_on_linked(
        trigger: On<'_, '_, Add<Linked>>,
        query: Query<'_, '_, (), With<Server>>,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            trace!("Server Linked! Adding Started");
            commands.entity(trigger.entity).try_insert(Started);
        }
    }

    fn connecting_on_link_of_linking(
        trigger: On<'_, '_, Add<Linking>>,
        query: Query<'_, '_, (), With<LinkOf>>,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            commands.entity(trigger.entity).try_insert(Connecting);
        }
    }

    /// A `RemoteId` the IO layer already put on the link is kept: a connection-oriented IO layer
    /// can carry several links from one `RemoteAddress`, so it names each peer itself.
    fn connected_on_link_of_linked(
        trigger: On<'_, '_, Add<Linked>>,
        query: Query<'_, '_, (&RemoteAddress, Option<&RemoteId>), With<LinkOf>>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok((remote_address, remote_id)) = query.get(trigger.entity) {
            trace!("LinkOf Linked! Adding Connected");
            commands.entity(trigger.entity).try_insert((
                Connected,
                LocalId(PeerId::Server),
                remote_id
                    .copied()
                    .unwrap_or(RemoteId(PeerId::Address(remote_address.0))),
                ClientOf,
            ));
        }
    }

    /// A `Stop` unlinks, which reports the stop's code. The server's link unlinks each of its
    /// links, which this plugin then ends.
    fn unlink_on_stop(
        trigger: On<'_, '_, Stop>,
        query: Query<'_, '_, (), (With<Server>, Without<Stopped>)>,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            trace!("Server Stop! Triggering Unlink");
            // Stopping goes first: an IO layer that unlinks at once inserts Stopped, which a later
            // Stopping would remove.
            commands.entity(trigger.entity).try_insert(Stopping);
            commands.trigger(Unlink {
                entity: trigger.entity,
                reason: UnlinkReason::Closed(trigger.reason),
            });
        }
    }

    /// Disconnects each `Disconnecting` link and despawns each `Disconnected` one
    /// (11-net-stack/connection.md, The connection layer ends every server-side link).
    fn end_links(
        disconnecting: Query<'_, '_, (Entity, &Disconnecting), With<LinkOf>>,
        disconnected: Query<'_, '_, Entity, (With<LinkOf>, With<Disconnected>)>,
        mut commands: Commands<'_, '_>,
    ) {
        for (entity, disconnecting) in &disconnecting {
            trace!(?entity, "Disconnecting link: Disconnected and despawned");
            commands.entity(entity).try_insert(Disconnected {
                reason: DisconnectedReason::UserRequested(disconnecting.reason),
            });
            commands.trigger(Unlink {
                entity,
                reason: UnlinkReason::Closed(disconnecting.reason),
            });
            commands.entity(entity).try_despawn();
        }
        for entity in &disconnected {
            trace!(?entity, "Despawning Disconnected link");
            commands.entity(entity).try_despawn();
        }
    }
}

impl Plugin for ServerConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(Self::stopped_on_add);
        app.add_observer(Self::start);
        app.add_observer(Self::stop_if_link_fails);
        app.add_observer(Self::disconnect);
        app.add_observer(Self::starting_on_linking);
        app.add_observer(Self::started_on_linked);
        app.add_observer(Self::connecting_on_link_of_linking);
        app.add_observer(Self::connected_on_link_of_linked);
        app.add_observer(Self::unlink_on_stop);
        app.add_systems(Last, Self::end_links);
    }
}

#[cfg(test)]
mod tests;
