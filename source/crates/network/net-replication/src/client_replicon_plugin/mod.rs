use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_state::prelude::*;

use bevy_replicon::client::confirm_history::ConfirmHistory;
use bevy_replicon::client::server_mutate_ticks::MutateTickReceived;
use bevy_replicon::prelude::*;
use campfire_net_connection::Client;
use campfire_net_connection::Connected;
use campfire_net_transport::Transport;
use campfire_net_transport::TransportSystems;

use crate::confirmed_tick_map::ConfirmedTickMap;
use crate::persistent::Persistent;
use crate::remote_parent::RemoteParent;
use crate::replication_receiver::ReplicationReceiver;
use crate::replication_systems::ReplicationSystems;
use crate::replicon_channel_map::RepliconChannelMap;
use campfire_net_despawns::DespawnReplicated;
use campfire_net_messages::MessageSystems;
use tracing::debug;
use tracing::error;

/// Runs Replicon's client on the stack: its state follows the connection, its messages go through
/// the transport, and a message names server entities through Replicon's `ServerEntityMap`, with no
/// copy for each link (11-net-stack/api.md, Only a client maps the entities a message names).
#[derive(Debug)]
pub(crate) struct ClientRepliconPlugin;

impl Plugin for ClientRepliconPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConfirmedTickMap>();
        app.add_observer(ConfirmedTickMap::record_authoritative_tick);
        app.add_observer(Self::on_replication_connect);
        // Replicon's reset and the confirmed ticks' clear run in one transition, so the two never
        // disagree on which ticks a link confirmed.
        app.add_systems(
            OnExit(ClientState::Connected),
            Self::forget_confirmed_ticks.in_set(ClientSystems::Reset),
        );

        // Packet bridge: replicon <-> the stack's transport
        app.add_systems(
            PreUpdate,
            Self::receive_client_packets.in_set(ClientSystems::ReceivePackets),
        );
        app.add_systems(
            PreUpdate,
            Self::sync_last_confirmed_tick
                .after(ClientSystems::Receive)
                .in_set(ReplicationSystems::Receive)
                .run_if(in_state(ClientState::Connected)),
        );
        app.add_systems(
            PostUpdate,
            Self::send_client_packets.in_set(ClientSystems::SendPackets),
        );

        // bevy_replicon's reset only clears the entity map; the stack must clean up the actual
        // entities when their receiver disconnects or is removed.
        app.add_observer(Self::on_replication_disconnect);

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

impl ClientRepliconPlugin {
    /// Maps each mutate tick Replicon confirms this frame to the tick it confirms, once.
    fn sync_last_confirmed_tick(
        mut confirmed: MessageReader<'_, '_, MutateTickReceived>,
        mut confirmed_ticks: ResMut<'_, ConfirmedTickMap>,
    ) {
        for &MutateTickReceived { tick } in confirmed.read() {
            if confirmed_ticks.record_last_confirmed_tick(tick).is_none() {
                error!(?tick, "missing confirmed tick for completed mutate tick");
            }
        }
    }

    /// Makes replicon's `ClientState` connected as a client that receives replication connects:
    /// the state follows the link's own events, so a link that fails and comes back between two
    /// frames still runs replicon's reset, as `NextState::set` runs the transition to the same
    /// state too.
    fn on_replication_connect(
        trigger: On<'_, '_, Add<(Connected, ReplicationReceiver)>>,
        receivers: Query<'_, '_, (), (With<Connected>, With<Client>, With<ReplicationReceiver>)>,
        mut next_state: ResMut<'_, NextState<ClientState>>,
    ) {
        if receivers.contains(trigger.entity) {
            next_state.set(ClientState::Connected);
        }
    }

    /// Forgets the confirmed ticks as replicon resets: none of the next link's ticks are the last
    /// one's.
    fn forget_confirmed_ticks(mut confirmed_ticks: ResMut<'_, ConfirmedTickMap>) {
        confirmed_ticks.clear();
    }

    /// Receive packets from transports and populate `ClientMessages` (replication data from
    /// server).
    ///
    /// Reads from server_channels (Updates, Mutations) on each transport; a client with no
    /// `ReplicationReceiver` applies none, so its messages are dropped, as replication alone reads
    /// its channels (11-net-stack/transport.md, A channel has one reader). A receiver's messages
    /// wait in its channels while replicon's state is not connected, or changes this frame: a link
    /// that came back gives its first messages to replicon once its reset ran, not before.
    fn receive_client_packets(
        channel_map: Res<'_, RepliconChannelMap>,
        state: Res<'_, State<ClientState>>,
        next_state: Res<'_, NextState<ClientState>>,
        mut client_messages: ResMut<'_, ClientMessages>,
        mut transports: Query<'_, '_, (&mut Transport, Has<ReplicationReceiver>), With<Client>>,
    ) {
        let admitted =
            *state.get() == ClientState::Connected && matches!(*next_state, NextState::Unchanged);
        for (mut transport, receives) in transports.iter_mut() {
            if receives && !admitted {
                continue;
            }
            for (idx, channel) in channel_map.server_channels.iter().enumerate() {
                if let Some(receiver) = transport.channel_receiver_mut(*channel) {
                    while let Some(message) = receiver.read_message() {
                        if receives {
                            client_messages.insert_received(idx, message.bytes);
                        }
                    }
                }
            }
        }
    }

    /// Send `ClientMessages` (acks) via transport to server.
    ///
    /// Drains `ClientMessages` and sends on client_channels (MutationAcks). Replicon's client
    /// holds one server's ticks and entities, so a client app has one link that receives
    /// replication at most.
    fn send_client_packets(
        channel_map: Res<'_, RepliconChannelMap>,
        mut client_messages: ResMut<'_, ClientMessages>,
        mut transports: Query<'_, '_, &mut Transport, (With<Client>, With<ReplicationReceiver>)>,
    ) {
        debug_assert!(
            transports.iter().len() <= 1,
            "a client app has one link that receives replication at most"
        );
        for (channel_idx, message) in client_messages.drain_sent() {
            let channel_id = channel_map.client_channels[channel_idx];
            for mut transport in &mut transports {
                transport.send(channel_id, message.clone(), 1.0);
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
    /// Replicon's state becomes disconnected, so its reset runs in the frame's transition. Each
    /// received entity is despawned, except a [`Persistent`] one, which becomes a local
    /// entity: its replication markers go, as replicon forgets which server entity it was. A
    /// reconnect then replicates the server's entity anew, as a second entity.
    fn on_replication_disconnect(
        trigger: On<'_, '_, Remove<(Connected, ReplicationReceiver)>>,
        mut next_state: ResMut<'_, NextState<ClientState>>,
        mut commands: Commands<'_, '_>,
        receivers: Query<
            '_,
            '_,
            Has<Persistent>,
            (With<Connected>, With<Client>, With<ReplicationReceiver>),
        >,
        replicated: Query<'_, '_, (Entity, Has<Persistent>), With<Remote>>,
    ) {
        // The tuple observer runs when either component is removed. Only clean up a connection that
        // had both components immediately before this removal, and do not clean up twice if its
        // receiver entity is later despawned.
        let Ok(receiver_is_persistent) = receivers.get(trigger.entity) else {
            return;
        };

        next_state.set(ClientState::Disconnected);

        // The despawns come first: each detaches its `Remote` children, a persistent one among
        // them, which then becomes local with its parent gone.
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
}

#[cfg(test)]
mod tests;
