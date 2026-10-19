use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_state::prelude::*;

use bevy_replicon::client::confirm_history::ConfirmHistory;
use bevy_replicon::client::server_mutate_ticks::ServerMutateTicks;
use bevy_replicon::prelude::*;
use campfire_net_connection::Client;
use campfire_net_connection::Connected;
use campfire_net_connection::HostClient;
use campfire_net_transport::Transport;
use campfire_net_transport::TransportSystems;

use crate::checkpoint;
use crate::persistent::Persistent;
use crate::remote_parent::RemoteParent;
use crate::replication_checkpoint_map::ReplicationCheckpointMap;
use crate::replication_receiver::ReplicationReceiver;
use crate::replication_systems::ReplicationSystems;
use crate::replicon_channel_map::RepliconChannelMap;
use campfire_net_despawn::DespawnReplicated;
use campfire_net_messages::MessageSystems;
use tracing::debug;
use tracing::error;

/// Adds the replicon client-side backend bridge for the stack.
///
/// Handles:
/// - `ClientState` transitions (Connected when client connects)
/// - Receiving `ClientMessages` (replication data from server) via transport
/// - Sending `ClientMessages` (acks) via transport
/// - Marking `Client` connections to resolve message entity mappings through
///   replicon's `ServerEntityMap` (no per-connection copy is kept)
#[derive(Debug)]
pub(crate) struct ClientRepliconPlugin;

impl Plugin for ClientRepliconPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(checkpoint::record_authoritative_tick_userdata);

        // State management
        app.add_systems(
            PreUpdate,
            sync_client_state.before(ClientSystems::ReceivePackets),
        );

        // Packet bridge: replicon <-> the stack's transport
        app.add_systems(
            PreUpdate,
            receive_client_packets.in_set(ClientSystems::ReceivePackets),
        );
        app.add_systems(
            PreUpdate,
            sync_last_confirmed_checkpoint
                .after(ClientSystems::Receive)
                .in_set(ReplicationSystems::Receive),
        );
        app.add_systems(
            PostUpdate,
            send_client_packets.in_set(ClientSystems::SendPackets),
        );

        // bevy_replicon's reset only clears the entity map; the stack must clean up the actual
        // entities when their receiver disconnects or is removed.
        app.add_observer(on_replication_disconnect);

        app.configure_sets(
            PreUpdate,
            ClientSystems::ReceivePackets
                .after(TransportSystems::Receive)
                .before(MessageSystems::Receive),
        );
        // Replicon applies the received replication in its receive set, which the stack's
        // receive set holds, so that a system ordered after it sees what arrived this frame.
        app.configure_sets(
            PreUpdate,
            ClientSystems::Receive.in_set(ReplicationSystems::Receive),
        );
        app.configure_sets(
            PostUpdate,
            ClientSystems::SendPackets.before(TransportSystems::Send),
        );
    }
}

fn sync_last_confirmed_checkpoint(
    server_mutate_ticks: Res<'_, ServerMutateTicks>,
    mut checkpoints: ResMut<'_, ReplicationCheckpointMap>,
    connected_receivers: Query<
        '_,
        '_,
        (),
        (
            With<Connected>,
            With<Client>,
            With<ReplicationReceiver>,
            Without<HostClient>,
        ),
    >,
) {
    // Receiver cleanup clears the checkpoint map before Replicon's state transition resets its
    // mutate ticks. Do not compare those two resources during that short transition window.
    if connected_receivers.is_empty() {
        return;
    }

    let Some(replicon_tick) = server_mutate_ticks.last_confirmed_tick() else {
        return;
    };
    if checkpoints
        .record_last_confirmed_checkpoint(replicon_tick)
        .is_none()
    {
        error!(
            ?replicon_tick,
            "missing authoritative checkpoint mapping for completed mutate tick"
        );
    }
}

/// Sync replicon's `ClientState` with the stack's lifecycle.
///
/// Sets `Connected` only for real remote clients that can receive replication.
///
/// Host-clients intentionally keep Replicon's `ClientState` disconnected so the app behaves like
/// a listen server: replication receive stays disabled and host-local client behavior is emulated
/// directly in the shared world instead.
fn sync_client_state(
    connected: Query<
        '_,
        '_,
        (),
        (
            With<Connected>,
            With<Client>,
            With<ReplicationReceiver>,
            Without<HostClient>,
        ),
    >,
    state: Res<'_, State<ClientState>>,
    mut next_state: ResMut<'_, NextState<ClientState>>,
) {
    if !connected.is_empty() && *state.get() != ClientState::Connected {
        next_state.set(ClientState::Connected);
    }
    if connected.is_empty() && *state.get() != ClientState::Disconnected {
        next_state.set(ClientState::Disconnected);
    }
}

/// Receive packets from transports and populate `ClientMessages` (replication data from server).
///
/// Reads from server_channels (Updates, Mutations) on each transport.
fn receive_client_packets(
    channel_map: Res<'_, RepliconChannelMap>,
    mut client_messages: ResMut<'_, ClientMessages>,
    mut transports: Query<'_, '_, &mut Transport, (With<Client>, With<ReplicationReceiver>)>,
) {
    for mut transport in transports.iter_mut() {
        for (idx, channel) in channel_map.server_channels.iter().enumerate() {
            if let Some(receiver) = transport.channel_receiver_mut(*channel) {
                while let Some(message) = receiver.read_message() {
                    client_messages.insert_received(idx, message.bytes);
                }
            }
        }
    }
}

/// Send `ClientMessages` (acks) via transport to server.
///
/// Drains `ClientMessages` and sends on client_channels (MutationAcks).
fn send_client_packets(
    channel_map: Res<'_, RepliconChannelMap>,
    mut client_messages: ResMut<'_, ClientMessages>,
    mut transports: Query<'_, '_, &mut Transport, (With<Client>, With<ReplicationReceiver>)>,
) {
    for (channel_idx, message) in client_messages.drain_sent() {
        let channel_id = channel_map.client_channels[channel_idx];
        for mut transport in &mut transports {
            if let Err(error) = transport.send(channel_id, message.clone(), 1.0) {
                error!(?error, "could not send a replication message");
            }
        }
    }
}

/// Cleans up receiver-side replication state when a receiver disconnects or is removed.
///
/// Watching `Remove` for both [`Connected`] and [`ReplicationReceiver`] handles disconnection,
/// explicit receiver removal, and receiver entity despawn with a single observer. Lifecycle
/// `Remove` observers run before the components disappear, so receiver-local [`Persistent`] can
/// still be read here.
///
/// Each received entity is despawned, except a [`Persistent`] one, which becomes a local entity:
/// its replication markers go, as replicon forgets which server entity it was. A reconnect then
/// replicates the server's entity anew, as a second entity.
fn on_replication_disconnect(
    trigger: On<'_, '_, Remove<(Connected, ReplicationReceiver)>>,
    mut commands: Commands<'_, '_>,
    receivers: Query<
        '_,
        '_,
        Has<Persistent>,
        (
            With<Connected>,
            With<Client>,
            With<ReplicationReceiver>,
            Without<HostClient>,
        ),
    >,
    replicated: Query<'_, '_, (Entity, Has<Persistent>), With<Remote>>,
    mut checkpoints: ResMut<'_, ReplicationCheckpointMap>,
) {
    // The tuple observer runs when either component is removed. Only clean up a connection that
    // had both components immediately before this removal, and do not clean up twice if its
    // receiver entity is later despawned.
    let Ok(receiver_is_persistent) = receivers.get(trigger.entity) else {
        return;
    };

    checkpoints.clear();

    // The despawns come first: each detaches its `Remote` children, a persistent one among them,
    // which then becomes local with its parent gone.
    for (entity, is_persistent) in &replicated {
        if !(receiver_is_persistent || is_persistent) {
            debug!("Despawning replicated entity {:?} on disconnect", entity);
            commands.entity(entity).queue_silenced(DespawnReplicated);
        }
    }
    for (entity, is_persistent) in &replicated {
        if receiver_is_persistent || is_persistent {
            debug!(
                "Keeping persistent replicated entity {:?} as a local entity",
                entity
            );
            commands
                .entity(entity)
                .try_remove::<(Remote, ConfirmHistory, RemoteParent)>();
        }
    }
}

#[cfg(test)]
mod tests;
