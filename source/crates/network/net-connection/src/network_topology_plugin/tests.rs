use super::*;

use crate::connection_plugins::ConnectionPlugins;
use crate::connection_state::DisconnectedReason;
use campfire_net_payload::AppCode;

use crate::remote_id::RemoteId;
use campfire_net_core::PeerId;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(ConnectionPlugins);
    app.update();
    app
}

fn connect_client(app: &mut App, peer: u64) -> Entity {
    app.world_mut()
        .spawn((Client, RemoteId(PeerId::Local(peer)), Connected))
        .id()
}

fn start_server(app: &mut App) -> Entity {
    app.world_mut().spawn((Server::default(), Started)).id()
}

fn mode(app: &App) -> &NetworkTopology {
    &app.world().resource::<NetworkingState>().topology
}

#[test]
fn only_a_ready_client_or_server_is_cached() {
    let mut app = app();
    let client = app.world_mut().spawn(Client).id();
    app.update();
    assert_eq!(mode(&app), &NetworkTopology::Undefined);

    app.world_mut()
        .entity_mut(client)
        .insert((RemoteId(PeerId::Local(1)), Connected));
    app.update();
    assert_eq!(mode(&app), &NetworkTopology::Client(client));

    app.world_mut().entity_mut(client).insert(Disconnected {
        reason: DisconnectedReason::UserRequested(AppCode::new(AppCode::FIRST)),
    });
    app.update();
    assert_eq!(mode(&app), &NetworkTopology::Undefined);

    let server = app.world_mut().spawn(Server::default()).id();
    app.update();
    assert_eq!(mode(&app), &NetworkTopology::Undefined);

    app.world_mut().entity_mut(server).insert(Started);
    app.update();
    assert_eq!(mode(&app), &NetworkTopology::Server(server));

    app.world_mut()
        .entity_mut(server)
        .insert(Stopped::default());
    app.update();
    assert_eq!(mode(&app), &NetworkTopology::Undefined);
}

#[test]
fn host_client_requires_a_connected_client_and_its_started_server() {
    let mut app = app();
    let server = app.world_mut().spawn(Server::default()).id();
    let client = app
        .world_mut()
        .spawn((
            Client,
            RemoteId(PeerId::Local(0)),
            Connected,
            LinkOf { server },
            HostClient { buffer: Vec::new() },
        ))
        .id();

    app.update();
    assert_eq!(
        mode(&app),
        &NetworkTopology::Invalid(NetworkTopologyError::HostClientServerNotStarted {
            client,
            server
        })
    );

    app.world_mut().entity_mut(server).insert(Started);
    app.update();
    assert_eq!(mode(&app), &NetworkTopology::HostClient { server, client });
}

#[test]
fn an_unsupported_mix_of_ready_sides_is_invalid() {
    let mut app = app();
    let first = connect_client(&mut app, 1);
    let second = connect_client(&mut app, 2);
    app.update();
    assert_eq!(
        mode(&app),
        &NetworkTopology::Invalid(NetworkTopologyError::MultipleConnectedClients(
            SmallVec::from_slice(&[first, second])
        ))
    );

    app.world_mut().despawn(second);
    let server = start_server(&mut app);
    app.update();
    assert_eq!(
        mode(&app),
        &NetworkTopology::Invalid(NetworkTopologyError::MixedClientServer {
            client: first,
            server,
        })
    );

    let other_server = start_server(&mut app);
    app.world_mut().despawn(first);
    app.update();
    assert_eq!(
        mode(&app),
        &NetworkTopology::Invalid(NetworkTopologyError::MultipleStartedServers(
            SmallVec::from_slice(&[server, other_server])
        ))
    );
}

#[test]
fn a_connected_host_client_without_its_server_link_is_invalid() {
    let mut app = app();
    let client = app
        .world_mut()
        .spawn((
            Client,
            RemoteId(PeerId::Local(0)),
            Connected,
            HostClient { buffer: Vec::new() },
        ))
        .id();

    app.update();
    assert_eq!(
        mode(&app),
        &NetworkTopology::Invalid(NetworkTopologyError::HostClientMissingLinkOf { client })
    );
}

#[test]
fn an_unchanged_invalid_topology_leaves_the_networking_state_unchanged() {
    let mut app = app();
    let last_changed = app.world().resource_ref::<NetworkingState>().last_changed();

    // This invalidates the cache, but a disconnected Client does not change the mode.
    app.world_mut().spawn(Client);
    app.update();

    assert_eq!(
        app.world().resource_ref::<NetworkingState>().last_changed(),
        last_changed
    );
}
