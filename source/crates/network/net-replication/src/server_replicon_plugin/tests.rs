use super::*;
use bevy_ecs::error;
use bevy_replicon::shared::backend::connected_client::NetworkIdMap;
use bevy_state::app::StatesPlugin;
use campfire_net_channels::fragment_len_for_max_payload;
use campfire_net_connection::NetworkingState;
use campfire_net_connection::RemoteId;
use campfire_net_connection::Stopped;
use campfire_net_core::PeerId;
use campfire_net_despawns::DespawnReplicated;
use campfire_net_link::{Link, Linked, Server};
use campfire_net_transport::TransportPlugin;
use core::net::SocketAddr;

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

/// Peer ids of each kind whose bits can be equal, `Entity(1)`, the address of port 1 and
/// `Server`, share no `NetworkId`: a count gives them 0, 1 and 2.
#[test]
fn each_link_takes_its_own_network_id() {
    let mut app = App::new();
    app.add_observer(ServerRepliconPlugin::on_client_connected);
    app.init_resource::<NetworkIdMap>();

    let ids: Vec<_> = [
        PeerId::Entity(1),
        PeerId::Address(SocketAddr::from(([0, 0, 0, 0], 1))),
        PeerId::Server,
    ]
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
