use super::*;

use crate::client_connection_plugin::ClientConnectionPlugin;

use crate::networking_state::NetworkingState;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((ClientConnectionPlugin, HostPlugin));
    app
}

fn spawn_host(app: &mut App) -> (Entity, Entity) {
    let server = app.world_mut().spawn(Server::default()).id();
    let client = app.world_mut().spawn((Client, LinkOf { server })).id();
    (server, client)
}

#[test]
fn connect_is_retried_when_server_starts() {
    let mut app = app();
    let (server, client) = spawn_host(&mut app);

    app.world_mut().trigger(Connect { entity: client });
    app.world_mut().flush();

    assert!(app.world().entity(client).contains::<Connecting>());
    assert!(!app.world().entity(client).contains::<Connected>());

    app.world_mut().entity_mut(server).insert(Started);
    app.world_mut().flush();

    assert!(app.world().entity(client).contains::<Connected>());
    assert!(!app.world().entity(client).contains::<Connecting>());
    assert!(app.world().entity(client).contains::<HostClient>());
}

#[test]
fn disconnect_cancels_pending_connection() {
    let mut app = app();
    let (server, client) = spawn_host(&mut app);

    app.world_mut().trigger(Connect { entity: client });
    app.world_mut().flush();
    app.world_mut().trigger(Disconnect::from(client));
    app.world_mut().flush();

    assert!(app.world().entity(client).contains::<Disconnected>());
    assert!(!app.world().entity(client).contains::<Connecting>());

    app.world_mut().entity_mut(server).insert(Started);
    app.world_mut().flush();

    assert!(!app.world().entity(client).contains::<Connected>());
    assert!(!app.world().entity(client).contains::<HostClient>());
}

#[test]
fn connected_client_becomes_host_when_server_starts() {
    let mut app = app();
    let (server, client) = spawn_host(&mut app);
    app.world_mut().entity_mut(client).insert((
        LocalId(PeerId::Local(0)),
        RemoteId(PeerId::Local(0)),
        Connected,
    ));
    app.world_mut().flush();

    assert!(!app.world().entity(client).contains::<HostClient>());

    app.world_mut().entity_mut(server).insert(Started);
    app.world_mut().flush();

    assert!(app.world().entity(client).contains::<HostClient>());
    assert_eq!(
        app.world()
            .resource::<NetworkingState>()
            .peer_map
            .get(&PeerId::Local(0)),
        Some(&client)
    );
}
