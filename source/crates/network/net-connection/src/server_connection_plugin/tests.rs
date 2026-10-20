#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems and observers take their parameters by value"
)]

use super::*;
use crate::client_connection_plugin::ClientConnectionPlugin;
use crate::networking_state::NetworkingState;
use crate::server_state::StopReason;
use campfire_net_link::ServerLinkPlugin;
use campfire_net_payload::AppCode;

#[derive(Debug, Resource, Default)]
struct Ends {
    disconnected: Vec<(Entity, DisconnectedReason)>,
    unlinked: Vec<(Entity, UnlinkReason)>,
}

fn record_disconnected(
    trigger: On<'_, '_, Add<Disconnected>>,
    query: Query<'_, '_, &Disconnected>,
    mut ends: ResMut<'_, Ends>,
) {
    let reason = query.get(trigger.entity).unwrap().reason;
    ends.disconnected.push((trigger.entity, reason));
}

fn record_unlink(trigger: On<'_, '_, Unlink>, mut ends: ResMut<'_, Ends>) {
    ends.unlinked.push((trigger.entity, trigger.reason));
}

fn server_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        ServerLinkPlugin,
        ClientConnectionPlugin,
        ServerConnectionPlugin,
    ));
    app.init_resource::<Ends>();
    app.add_observer(record_disconnected);
    app.add_observer(record_unlink);
    app
}

/// Spawns a started server with a remote client's link.
fn spawn_link(app: &mut App) -> Entity {
    let server = app.world_mut().spawn((Server::default(), Started)).id();
    app.world_mut()
        .spawn((
            LinkOf { server },
            ClientOf,
            RemoteId(PeerId::Entity(1)),
            Connected,
        ))
        .id()
}

/// A remote client's link that disconnects, here as its link times out, is despawned at the end
/// of the frame, and its peer is unmapped.
#[test]
fn a_disconnected_link_is_despawned() {
    let mut app = server_app();
    let remote = spawn_link(&mut app);

    app.world_mut().entity_mut(remote).insert(Unlinked {
        reason: UnlinkReason::TimedOut,
    });
    app.update();

    assert!(app.world().get_entity(remote).is_err());
    let ends = app.world().resource::<Ends>();
    assert_eq!(
        ends.disconnected,
        [(
            remote,
            DisconnectedReason::LinkFailed(UnlinkReason::TimedOut)
        )]
    );
    let peer_map = &app.world().resource::<NetworkingState>().peer_map;
    assert!(!peer_map.contains_key(&PeerId::Entity(1)));
}

/// A `Disconnect` on a server's link waits for the end of the frame, then reports its code as
/// requested, closes the link with it, and despawns the link.
#[test]
fn disconnect_on_a_server_link_ends_it_with_its_code() {
    let mut app = server_app();
    let remote = spawn_link(&mut app);
    let code = AppCode::new(AppCode::FIRST + 2);

    app.world_mut().trigger(Disconnect {
        entity: remote,
        reason: code,
    });
    app.world_mut().flush();
    assert_eq!(
        app.world()
            .get::<Disconnecting>(remote)
            .map(|disconnecting| disconnecting.reason),
        Some(code)
    );

    app.update();

    assert!(app.world().get_entity(remote).is_err());
    let ends = app.world().resource::<Ends>();
    assert_eq!(
        ends.disconnected,
        [(remote, DisconnectedReason::UserRequested(code))]
    );
    assert_eq!(ends.unlinked, [(remote, UnlinkReason::Closed(code))]);
}

/// A started server whose own link fails stops with the link's reason, a link it closed stops
/// as requested with its code, and a server already stopped keeps the reason it stopped with.
#[test]
fn a_server_stops_when_its_link_fails() {
    let mut app = server_app();
    let code = AppCode::new(AppCode::FIRST);
    // A server spawns unlinked: each links, and starts or stops, before its link fails.
    let failed = app.world_mut().spawn(Server::default()).id();
    let closed = app.world_mut().spawn(Server::default()).id();
    let stopped = app.world_mut().spawn(Server::default()).id();
    app.update();
    for server in [failed, closed, stopped] {
        app.world_mut().entity_mut(server).insert((Linked, Started));
    }
    app.update();
    app.world_mut().entity_mut(stopped).insert(Stopped {
        reason: StopReason::UserRequested(code),
    });
    app.update();
    for (server, reason) in [
        (failed, UnlinkReason::SocketFailed),
        (closed, UnlinkReason::Closed(code)),
        (stopped, UnlinkReason::SocketFailed),
    ] {
        app.world_mut()
            .entity_mut(server)
            .insert(Unlinked { reason });
    }
    app.update();

    let reason = |server| {
        app.world()
            .get::<Stopped>(server)
            .map(|stopped| stopped.reason)
    };
    assert_eq!(
        reason(failed),
        Some(StopReason::LinkFailed(UnlinkReason::SocketFailed))
    );
    assert_eq!(reason(closed), Some(StopReason::UserRequested(code)));
    assert_eq!(reason(stopped), Some(StopReason::UserRequested(code)));
}

/// A peer that reconnects is mapped to its new link, and the old link's disconnect leaves that
/// mapping.
#[test]
fn a_reconnect_keeps_its_new_mapping() {
    let mut app = server_app();
    let old = spawn_link(&mut app);
    let server = app.world().get::<LinkOf>(old).unwrap().server;
    let new = app
        .world_mut()
        .spawn((
            LinkOf { server },
            ClientOf,
            RemoteId(PeerId::Entity(1)),
            Connected,
        ))
        .id();

    app.world_mut()
        .entity_mut(old)
        .insert(Disconnected::default());
    app.update();

    assert_eq!(
        app.world()
            .resource::<NetworkingState>()
            .peer_map
            .get(&PeerId::Entity(1)),
        Some(&new)
    );
}

/// `peer_map` names the server while it is `Started`: going down already unmaps it.
#[test]
fn a_server_stays_in_the_peer_map_while_it_is_started() {
    let mut app = App::new();
    app.init_resource::<NetworkingState>();
    let server = app.world_mut().spawn((Server::default(), Started)).id();

    assert_eq!(
        app.world()
            .resource::<NetworkingState>()
            .peer_map
            .get(&PeerId::Server),
        Some(&server)
    );

    app.world_mut().entity_mut(server).insert(Stopping);

    assert!(
        !app.world()
            .resource::<NetworkingState>()
            .peer_map
            .contains_key(&PeerId::Server)
    );
}

#[test]
fn a_despawned_server_leaves_the_peer_map() {
    let mut app = App::new();
    app.init_resource::<NetworkingState>();
    let server = app.world_mut().spawn((Server::default(), Started)).id();

    assert_eq!(
        app.world()
            .resource::<NetworkingState>()
            .peer_map
            .get(&PeerId::Server),
        Some(&server)
    );

    app.world_mut().despawn(server);

    assert!(
        !app.world()
            .resource::<NetworkingState>()
            .peer_map
            .contains_key(&PeerId::Server)
    );
}

fn despawn_on_unlinked(trigger: On<'_, '_, Add<Unlinked>>, mut commands: Commands<'_, '_>) {
    commands.entity(trigger.entity).despawn();
}

/// An IO that despawns its own link once it unlinks races the connection layer's disconnect and
/// despawn of it in the same flush, which then do nothing.
#[test]
fn an_io_that_despawns_its_own_link_is_tolerated() {
    let mut app = server_app();
    app.add_observer(despawn_on_unlinked);
    let remote = spawn_link(&mut app);

    app.world_mut().entity_mut(remote).insert(Unlinked {
        reason: UnlinkReason::TimedOut,
    });
    app.update();

    assert!(app.world().get_entity(remote).is_err());
    assert!(
        !app.world()
            .resource::<NetworkingState>()
            .peer_map
            .contains_key(&PeerId::Entity(1))
    );
}

/// A `Stop` stops the server with its code, and each link of the server disconnects as its
/// server stopped, then goes at the end of the frame.
#[test]
fn a_stop_reports_its_code_and_ends_each_link_of_the_server() {
    let mut app = App::new();
    app.add_plugins((
        campfire_net_link::LinkPlugin,
        ServerLinkPlugin,
        ClientConnectionPlugin,
        ServerConnectionPlugin,
    ));
    let server = app.world_mut().spawn((Server::default(), Linked)).id();
    let client = app
        .world_mut()
        .spawn((
            LinkOf { server },
            RemoteAddress("127.0.0.1:5000".parse().unwrap()),
            Linked,
        ))
        .id();
    app.world_mut().flush();
    assert!(app.world().entity(server).contains::<Started>());
    assert!(app.world().entity(client).contains::<Connected>());

    let code = AppCode::new(AppCode::FIRST + 3);
    app.world_mut().trigger(Stop {
        entity: server,
        reason: code,
    });
    app.world_mut().flush();

    assert_eq!(
        app.world()
            .get::<Stopped>(server)
            .map(|stopped| stopped.reason),
        Some(StopReason::UserRequested(code))
    );
    assert_eq!(
        app.world()
            .get::<Disconnected>(client)
            .map(|disconnected| disconnected.reason),
        Some(DisconnectedReason::LinkFailed(UnlinkReason::ServerStopped))
    );
    app.update();
    assert!(app.world().get_entity(client).is_err());
    assert!(app.world().get_entity(server).is_ok());
}

/// A server starts stopped unless it spawns in another state, which its link's state backs.
#[test]
fn a_server_starts_stopped_unless_it_spawns_linked_and_started() {
    let mut app = App::new();
    app.init_resource::<NetworkingState>();
    app.add_plugins(ServerConnectionPlugin);
    let fresh = app.world_mut().spawn(Server::default()).id();
    let started = app
        .world_mut()
        .spawn((Server::default(), Linked, Started))
        .id();
    app.world_mut().flush();

    assert_eq!(
        app.world()
            .get::<Stopped>(fresh)
            .map(|stopped| stopped.reason),
        Some(StopReason::Initial)
    );
    let started = app.world().entity(started);
    assert!(started.contains::<Started>());
    assert!(!started.contains::<Stopped>());
}

#[test]
fn a_server_link_keeps_the_remote_id_its_io_layer_set() {
    let mut app = App::new();
    app.add_plugins((ClientConnectionPlugin, ServerConnectionPlugin));
    let server = app.world_mut().spawn(Server::default()).id();
    let address = "127.0.0.1:5000".parse().unwrap();
    let named = RemoteId(PeerId::Entity(7));
    let named_link = app
        .world_mut()
        .spawn((LinkOf { server }, RemoteAddress(address), named))
        .id();
    let addressed_link = app
        .world_mut()
        .spawn((LinkOf { server }, RemoteAddress(address)))
        .id();

    app.world_mut().entity_mut(named_link).insert(Linked);
    app.world_mut().entity_mut(addressed_link).insert(Linked);
    app.world_mut().flush();

    let named_link = app.world().entity(named_link);
    assert!(named_link.contains::<Connected>());
    assert_eq!(named_link.get::<RemoteId>(), Some(&named));
    let addressed_link = app.world().entity(addressed_link);
    assert!(addressed_link.contains::<Connected>());
    assert_eq!(
        addressed_link.get::<RemoteId>(),
        Some(&RemoteId(PeerId::Address(address)))
    );
}
