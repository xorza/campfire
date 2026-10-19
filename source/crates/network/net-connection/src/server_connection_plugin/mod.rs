use crate::client::Client;
use crate::client_of::ClientOf;
use crate::connection_state::Disconnected;
use crate::connection_state::DisconnectedReason;
use crate::connection_state::Disconnecting;
use crate::disconnect::Disconnect;
use crate::server_state::Stopped;
use crate::start::Start;
use bevy_app::{App, Last, Plugin};
use bevy_ecs::prelude::*;
use campfire_net_link::{LinkOf, Server};
use campfire_net_link::{LinkStart, Unlink, Unlinked};
use campfire_net_payload::UnlinkReason;
use core::fmt::Debug;
use tracing::trace;

/// Starts and stops servers with their links, and ends every server-side link: a link that is
/// not a host client is despawned once it is [`Disconnected`], for any cause, and a reconnect is
/// a new entity.
#[derive(Debug)]
pub(crate) struct ServerConnectionPlugin;

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
        query: Query<'_, '_, (), (With<ClientOf>, Without<Client>, Without<Disconnected>)>,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            commands.entity(trigger.entity).try_insert(Disconnecting {
                reason: trigger.reason,
            });
        }
    }

    /// Disconnects each `Disconnecting` link and despawns each `Disconnected` one, except a host
    /// client's, which is the client itself.
    fn end_links(
        disconnecting: Query<'_, '_, (Entity, &Disconnecting), With<LinkOf>>,
        disconnected: Query<'_, '_, Entity, (With<LinkOf>, With<Disconnected>, Without<Client>)>,
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
        app.add_observer(Self::start);
        app.add_observer(Self::stop_if_link_fails);
        app.add_observer(Self::disconnect);
        app.add_systems(Last, Self::end_links);
    }
}

#[cfg(test)]
mod tests;
