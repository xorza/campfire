use crate::authoritative_tick::encode_authoritative_tick;
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_replicon::server::ReplicationUserdata;
use bevy_state::prelude::*;
use campfire_net_timeline::LocalTimeline;

use bevy_replicon::prelude::*;
use bevy_replicon::server::visibility::client_visibility::ClientVisibility;
use bevy_replicon::shared::backend::connected_client::NetworkId;
use campfire_net_connection::ClientOf;
use campfire_net_connection::Connected;
use campfire_net_connection::HostClient;
use campfire_net_connection::Started;
use campfire_net_transport::Transport;
use campfire_net_transport::TransportSystems;

use crate::detached_bit::DetachedBit;
use crate::replication_sender::ReplicationSender;
use crate::replicon_channel_map::RepliconChannelMap;
use campfire_net_messages::MessageSystems;
use tracing::{error, trace};

/// Runs Replicon's server on the stack: its state follows `Started`, each connected link is a
/// Replicon client, and its messages go through the transport.
#[derive(Debug)]
pub(crate) struct ServerRepliconPlugin;

impl Plugin for ServerRepliconPlugin {
    fn build(&self, app: &mut App) {
        // When Connected is added to a link entity, add replicon's ConnectedClient + NetworkId
        app.add_observer(Self::on_client_connected);
        app.add_observer(Self::on_host_client_added);
        app.add_observer(Self::on_host_client_removed);
        // Gate replication (not messaging) on ReplicationSender via ClientVisibility.
        app.init_resource::<DetachedBit>();
        app.add_observer(Self::strip_client_visibility_without_sender);
        app.add_observer(Self::admit_client_visibility_with_sender);
        app.add_observer(Self::detach_link_with_sender_removed);
        app.add_observer(Self::attach_detached_link_with_sender_added);
        app.add_observer(Self::hide_new_entity_from_detached_links);

        // State management
        app.add_observer(Self::on_server_started);
        app.add_observer(Self::on_server_stopped);

        // Packet bridge: replicon <-> the stack's transport
        app.add_systems(
            PreUpdate,
            Self::receive_server_packets.in_set(ServerSystems::ReceivePackets),
        );
        app.add_systems(
            PostUpdate,
            (
                Self::write_authoritative_tick_userdata.before(ServerSystems::Send),
                Self::send_server_packets.in_set(ServerSystems::SendPackets),
            ),
        );

        app.configure_sets(
            PreUpdate,
            ServerSystems::ReceivePackets
                .after(TransportSystems::Receive)
                // Replicon bridge must read its channels before the stack's MessagePlugin::receive
                // drains ALL transport receivers (including replicon channels)
                .before(MessageSystems::Receive),
        );
        app.configure_sets(
            PostUpdate,
            ServerSystems::SendPackets.before(TransportSystems::Send),
        );
    }
}

impl ServerRepliconPlugin {
    /// Write the authoritative network tick into Replicon's replication userdata.
    pub(crate) fn write_authoritative_tick_userdata(
        timeline: Res<'_, LocalTimeline>,
        mut userdata: ResMut<'_, ReplicationUserdata>,
    ) {
        userdata.clear();
        userdata.extend_from_slice(&encode_authoritative_tick(timeline.tick()));
    }

    /// When `Connected` is added to a remote client link entity, insert replicon's
    /// `ConnectedClient`, whose largest message is the transport's fragment size, and `NetworkId`
    /// so replicon's packet path can target it. They go with the link entity, which the connection
    /// layer despawns once it is disconnected, as replicon's backend contract asks.
    ///
    /// Host-clients intentionally do not become replicon `ConnectedClient`s because they share the
    /// same world as the server.
    ///
    /// Each link's `NetworkId` is the next of a count, unique for the app's life: a peer id does
    /// not fit 64 bits injectively, and a reconnect is a new client to replicon.
    fn on_client_connected(
        trigger: On<'_, '_, Add<Connected>>,
        remotes: Query<'_, '_, &Transport, (With<ClientOf>, Without<HostClient>)>,
        mut next_network_id: Local<'_, u64>,
        mut commands: Commands<'_, '_>,
    ) {
        let entity = trigger.entity;
        let Ok(transport) = remotes.get(entity) else {
            return;
        };
        let max_size = transport.fragment_len();
        let network_id = NetworkId::new(*next_network_id);
        *next_network_id = next_network_id
            .checked_add(1)
            .expect("an app connects fewer than 2^64 clients");
        commands
            .entity(entity)
            .try_insert((ConnectedClient { max_size }, network_id));
    }

    /// A host client needs only `ClientVisibility`, for the stack's same-app visibility hooks,
    /// while it is a host client.
    fn on_host_client_added(
        trigger: On<'_, '_, Add<HostClient>>,
        visibilities: Query<'_, '_, (), With<ClientVisibility>>,
        mut commands: Commands<'_, '_>,
    ) {
        if !visibilities.contains(trigger.entity) {
            commands
                .entity(trigger.entity)
                .try_insert(ClientVisibility::default());
        }
    }

    /// A client that stops being a host client stops receiving same-app visibility.
    fn on_host_client_removed(
        trigger: On<'_, '_, Remove<HostClient>>,
        mut commands: Commands<'_, '_>,
    ) {
        if trigger.trigger().new_archetype.is_some() {
            commands
                .entity(trigger.entity)
                .try_remove::<ClientVisibility>();
        }
    }

    /// Only links with [`ReplicationSender`](crate::replication_sender::ReplicationSender)
    /// participate in replication.
    ///
    /// `ClientVisibility` is what admits a link into replicon's send loop, so it is kept exactly on
    /// connected server-side links that also have `ReplicationSender`. Links without it stay
    /// connected (messages still flow) but receive no replication. Host links keep their existing
    /// manual insert in [`ServerRepliconPlugin::on_client_connected`].
    fn strip_client_visibility_without_sender(
        trigger: On<'_, '_, Add<ClientVisibility>>,
        links: Query<
            '_,
            '_,
            (),
            (
                With<ClientOf>,
                Without<HostClient>,
                Without<ReplicationSender>,
            ),
        >,
        mut commands: Commands<'_, '_>,
    ) {
        if links.contains(trigger.entity) {
            commands.entity(trigger.entity).remove::<ClientVisibility>();
        }
    }

    /// Admit a connected link into replication once it gains
    /// [`ReplicationSender`](crate::replication_sender::ReplicationSender).
    ///
    /// Inserting `ClientVisibility` makes replicon evaluate each pre-existing entity for the link.
    fn admit_client_visibility_with_sender(
        trigger: On<'_, '_, Add<ReplicationSender>>,
        links: Query<
            '_,
            '_,
            (),
            (
                With<ClientOf>,
                Without<HostClient>,
                With<Connected>,
                Without<ClientVisibility>,
            ),
        >,
        mut commands: Commands<'_, '_>,
    ) {
        if links.contains(trigger.entity) {
            commands
                .entity(trigger.entity)
                .insert(ClientVisibility::default());
        }
    }

    /// A server-side link that loses
    /// [`ReplicationSender`](crate::replication_sender::ReplicationSender) while it replicates is
    /// detached: every replicated entity is hidden from it through the [`DetachedBit`], so its
    /// client despawns them, and the link keeps the rest of its Replicon state, manual visibility
    /// included.
    fn detach_link_with_sender_removed(
        trigger: On<'_, '_, Remove<ReplicationSender>>,
        detached_bit: Res<'_, DetachedBit>,
        mut links: Query<'_, '_, &mut ClientVisibility, (With<ClientOf>, Without<HostClient>)>,
        replicated: Query<'_, '_, Entity, With<Replicated>>,
    ) {
        if trigger.trigger().new_archetype.is_none() {
            return;
        }
        if let Ok(mut visibility) = links.get_mut(trigger.entity) {
            for entity in &replicated {
                visibility.set(entity, detached_bit.0, false);
            }
        }
    }

    /// A detached link that gains
    /// [`ReplicationSender`](crate::replication_sender::ReplicationSender) again sees every entity
    /// its other filters let through, each spawned anew.
    fn attach_detached_link_with_sender_added(
        trigger: On<'_, '_, Add<ReplicationSender>>,
        detached_bit: Res<'_, DetachedBit>,
        mut links: Query<'_, '_, &mut ClientVisibility, (With<ClientOf>, Without<HostClient>)>,
        replicated: Query<'_, '_, Entity, With<Replicated>>,
    ) {
        if let Ok(mut visibility) = links.get_mut(trigger.entity) {
            for entity in &replicated {
                visibility.set(entity, detached_bit.0, true);
            }
        }
    }

    /// An entity that starts replicating stays hidden from every detached link.
    fn hide_new_entity_from_detached_links(
        trigger: On<'_, '_, Add<Replicated>>,
        detached_bit: Res<'_, DetachedBit>,
        mut detached: Query<
            '_,
            '_,
            &mut ClientVisibility,
            (
                With<ClientOf>,
                Without<HostClient>,
                Without<ReplicationSender>,
            ),
        >,
    ) {
        for mut visibility in &mut detached {
            visibility.set(trigger.entity, detached_bit.0, false);
        }
    }

    /// Set replicon's `ServerState` to `Running` when the server starts.
    fn on_server_started(
        _trigger: On<'_, '_, Add<Started>>,
        mut next_state: ResMut<'_, NextState<ServerState>>,
    ) {
        next_state.set_if_different(ServerState::Running);
    }

    /// Set replicon's `ServerState` to `Stopped` when the last started server stops or is
    /// despawned.
    ///
    /// Bevy emits `Remove` when an entity is despawned, so this also handles teardown that bypasses
    /// the `Stopped` marker entirely.
    fn on_server_stopped(
        trigger: On<'_, '_, Remove<Started>>,
        started: Query<'_, '_, Entity, With<Started>>,
        mut next_state: ResMut<'_, NextState<ServerState>>,
    ) {
        if started.iter().all(|server| server == trigger.entity) {
            next_state.set_if_different(ServerState::Stopped);
        }
    }

    /// Receive packets from transports and populate `ServerMessages` (ack data from peers).
    ///
    /// Reads from client_channels (MutationAcks) on each transport and puts into `ServerMessages`.
    fn receive_server_packets(
        channel_map: Res<'_, RepliconChannelMap>,
        mut server_messages: ResMut<'_, ServerMessages>,
        mut transports: Query<'_, '_, (Entity, &mut Transport), With<ClientOf>>,
    ) {
        for (entity, mut transport) in transports.iter_mut() {
            for (idx, channel) in channel_map.client_channels.iter().enumerate() {
                if let Some(receiver) = transport.channel_receiver_mut(*channel) {
                    while let Some(message) = receiver.read_message() {
                        server_messages.insert_received(entity, idx, message.bytes);
                    }
                }
            }
        }
    }

    /// Send `ServerMessages` (replication data) via transport to peers.
    ///
    /// Drains `ServerMessages` and sends on server_channels (Updates, Mutations).
    fn send_server_packets(
        channel_map: Res<'_, RepliconChannelMap>,
        mut server_messages: ResMut<'_, ServerMessages>,
        mut transports: Query<'_, '_, &mut Transport, With<ClientOf>>,
    ) {
        for (client, channel_idx, message) in server_messages.drain_sent() {
            let channel_id = channel_map.server_channels[channel_idx];
            trace!(
                "ServerRepliconPlugin::send_server_packets: sending {} bytes on channel_idx={} to {:?}",
                message.len(),
                channel_idx,
                client
            );
            if let Ok(mut transport) = transports.get_mut(client) {
                if let Err(error) = transport.send(channel_id, message, 1.0) {
                    error!(?error, ?client, "could not send a replication message");
                }
            } else {
                trace!(
                    "ServerRepliconPlugin::send_server_packets: no transport for client {:?}",
                    client
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::error;
    use bevy_replicon::shared::backend::connected_client::NetworkIdMap;
    use bevy_state::app::StatesPlugin;
    use campfire_net_channels::fragment_len_for_max_payload;
    use campfire_net_connection::NetworkingState;
    use campfire_net_connection::RemoteId;
    use campfire_net_connection::Stopped;
    use campfire_net_core::PeerId;
    use campfire_net_despawn::DespawnReplicated;
    use campfire_net_link::{Link, Linked, Server};
    use campfire_net_transport::TransportPlugin;

    /// A link whose packet session carries payloads of at most `max_payload` bytes.
    fn link(max_payload: usize) -> Link {
        let mut link = Link::default();
        link.packet_session.set_max_payload(max_payload);
        link
    }

    /// The largest message is the fragment size the transport took at the link: 256 and 200
    /// bytes of payload give two sizes, and each client takes its own.
    #[test]
    fn a_connected_clients_max_len_is_the_transports_fragment_len() {
        let mut app = App::new();
        app.add_plugins(TransportPlugin);
        app.add_observer(ServerRepliconPlugin::on_client_connected);
        app.init_resource::<NetworkIdMap>();

        for (peer, max_payload) in [(1, 256), (2, 200)] {
            let entity = app
                .world_mut()
                .spawn((
                    RemoteId(PeerId::Entity(peer)),
                    ClientOf,
                    link(max_payload),
                    Transport::default(),
                    Linked,
                ))
                .id();
            app.world_mut().entity_mut(entity).insert(Connected);
            assert_eq!(
                app.world().get::<ConnectedClient>(entity).unwrap().max_size,
                fragment_len_for_max_payload(max_payload).unwrap(),
                "{max_payload}"
            );
        }
        assert_ne!(
            fragment_len_for_max_payload(256),
            fragment_len_for_max_payload(200)
        );
    }

    /// Peer ids `Entity(1)`, `Local(1)` and `Server` all gave the bits 1, so their links shared a
    /// `NetworkId`: a count gives them 0, 1 and 2.
    #[test]
    fn each_link_takes_its_own_network_id() {
        let mut app = App::new();
        app.add_observer(ServerRepliconPlugin::on_client_connected);
        app.init_resource::<NetworkIdMap>();

        let ids: Vec<_> = [PeerId::Entity(1), PeerId::Local(1), PeerId::Server]
            .into_iter()
            .map(|peer| {
                let entity = app
                    .world_mut()
                    .spawn((RemoteId(peer), ClientOf, Transport::default(), Connected))
                    .id();
                app.world_mut().flush();
                app.world().get::<NetworkId>(entity).unwrap().get()
            })
            .collect();

        assert_eq!(ids, [0, 1, 2]);
        assert_eq!(app.world().resource::<NetworkIdMap>().len(), 3);
    }

    fn app_with_server_state(state: ServerState) -> App {
        let mut app = App::new();
        app.add_plugins(StatesPlugin)
            .init_resource::<NetworkingState>()
            .init_state::<ServerState>()
            .add_observer(ServerRepliconPlugin::on_server_started)
            .add_observer(ServerRepliconPlugin::on_server_stopped)
            .insert_state(state);
        app
    }

    #[test]
    fn the_started_marker_moves_the_server_state() {
        let mut app = app_with_server_state(ServerState::Stopped);
        let server = app.world_mut().spawn((Server::default(), Started)).id();

        app.update();

        assert_eq!(
            *app.world().resource::<State<ServerState>>().get(),
            ServerState::Running
        );

        app.world_mut()
            .entity_mut(server)
            .insert(Stopped::default());

        app.update();

        assert_eq!(
            *app.world().resource::<State<ServerState>>().get(),
            ServerState::Stopped
        );
    }

    #[test]
    fn a_despawned_started_entity_stops_the_server_state() {
        let mut app = app_with_server_state(ServerState::Stopped);
        let server = app.world_mut().spawn((Server::default(), Started)).id();

        app.update();
        assert_eq!(
            *app.world().resource::<State<ServerState>>().get(),
            ServerState::Running
        );

        DespawnReplicated.apply(app.world_mut().entity_mut(server));

        app.update();

        assert_eq!(
            *app.world().resource::<State<ServerState>>().get(),
            ServerState::Stopped
        );
    }

    #[test]
    fn an_entity_started_and_despawned_in_one_flush_stays_stopped() {
        let mut app = app_with_server_state(ServerState::Stopped);
        let server = app.world_mut().spawn(Server::default()).id();
        app.world_mut()
            .commands()
            .entity(server)
            .insert(Started)
            .queue_handled(DespawnReplicated, error::warn);
        app.world_mut().flush();

        app.update();

        assert_eq!(
            *app.world().resource::<State<ServerState>>().get(),
            ServerState::Stopped
        );
    }

    /// Replication runs while any server is started: stopping one of two leaves it running, and
    /// stopping the last stops it.
    #[test]
    fn replication_stops_with_the_last_started_server() {
        let mut app = app_with_server_state(ServerState::Stopped);
        let servers = [(); 2].map(|()| app.world_mut().spawn((Server::default(), Started)).id());
        app.update();

        let state = |app: &App| *app.world().resource::<State<ServerState>>().get();
        app.world_mut()
            .entity_mut(servers[0])
            .insert(Stopped::default());
        app.update();
        assert_eq!(state(&app), ServerState::Running);

        DespawnReplicated.apply(app.world_mut().entity_mut(servers[1]));
        app.update();
        assert_eq!(state(&app), ServerState::Stopped);
    }

    /// A host client gets same-app visibility once connected, and loses it with its host side.
    #[test]
    fn a_host_client_has_visibility_while_it_is_one() {
        let mut app = App::new();
        app.add_observer(ServerRepliconPlugin::on_client_connected);
        app.add_observer(ServerRepliconPlugin::on_host_client_added);
        app.add_observer(ServerRepliconPlugin::on_host_client_removed);
        let host = app
            .world_mut()
            .spawn((
                RemoteId(PeerId::Local(0)),
                ClientOf,
                Connected,
                HostClient { buffer: Vec::new() },
            ))
            .id();
        app.world_mut().flush();
        assert!(app.world().entity(host).contains::<ClientVisibility>());
        assert!(!app.world().entity(host).contains::<ConnectedClient>());

        app.world_mut().entity_mut(host).remove::<HostClient>();
        app.world_mut().flush();
        assert!(!app.world().entity(host).contains::<ClientVisibility>());
    }
}
