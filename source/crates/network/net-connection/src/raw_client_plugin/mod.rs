use crate::connection_state::Connected;
use crate::connection_state::Connecting;
use crate::connection_state::Disconnected;
use crate::disconnect::Disconnect;
use crate::local_id::LocalId;
use crate::raw_client::RawClient;
use crate::remote_id::RemoteId;
use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use campfire_net_core::PeerId;
use campfire_net_link::LocalAddr;
use campfire_net_link::{Linked, Linking, Unlink};
use campfire_net_payload::UnlinkReason;
use tracing::trace;

#[derive(Debug)]
pub struct RawClientPlugin;

impl RawClientPlugin {
    /// For RawClients, Linking implies Connecting
    fn on_linking(
        trigger: On<'_, '_, Add<Linking>>,
        query: Query<'_, '_, (), With<RawClient>>,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            commands.entity(trigger.entity).try_insert(Connecting);
        }
    }

    /// For RawClients, Linked implies Connected
    fn on_linked(
        trigger: On<'_, '_, Add<Linked>>,
        query: Query<'_, '_, (&LocalAddr, Option<&LocalId>, Option<&RemoteId>), With<RawClient>>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok((local_addr, local_id, remote_id)) = query.get(trigger.entity) {
            trace!("RawClient Linked! Adding Connected");
            commands.entity(trigger.entity).try_insert((
                Connected,
                local_id
                    .copied()
                    .unwrap_or(LocalId(PeerId::Raw(local_addr.0))),
                remote_id.copied().unwrap_or(RemoteId(PeerId::Server)),
            ));
        }
    }

    /// For RawClients, Disconnect implies Unlink, which reports the disconnect's code
    fn on_disconnect(
        trigger: On<'_, '_, Disconnect>,
        mut commands: Commands<'_, '_>,
        query: Query<'_, '_, (), (Without<Disconnected>, With<RawClient>)>,
    ) {
        if query.contains(trigger.entity) {
            trace!("RawClient Disconnect! Triggering Unlink");
            commands.trigger(Unlink {
                entity: trigger.entity,
                reason: UnlinkReason::Closed(trigger.reason),
            });
        }
    }
}

impl Plugin for RawClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(Self::on_linking);
        app.add_observer(Self::on_linked);
        app.add_observer(Self::on_disconnect);
    }
}

#[cfg(test)]
mod tests;
