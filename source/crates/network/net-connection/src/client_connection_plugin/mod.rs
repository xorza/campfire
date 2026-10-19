use crate::connect::Connect;
use crate::connection_state::Disconnected;
use crate::networking_state::NetworkingState;
use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use campfire_net_link::LinkStart;
use campfire_net_link::{Server, Unlinked};
use tracing::trace;

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
}

impl Plugin for ClientConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NetworkingState>();
        app.add_observer(Self::connect);
        app.add_observer(Self::disconnect_if_link_fails);
    }
}

#[cfg(test)]
mod tests;
