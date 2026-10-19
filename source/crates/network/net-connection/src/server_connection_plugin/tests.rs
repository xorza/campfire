#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems and observers take their parameters by value"
)]

use super::*;
use crate::client_connection_plugin::ClientConnectionPlugin;
use crate::connection_state::Connected;
use crate::networking_state::NetworkingState;
use crate::remote_id::RemoteId;
use crate::server_state::Started;
use crate::server_state::StopReason;
use crate::server_state::Stopping;
use campfire_net_core::PeerId;
use campfire_net_link::Linked;
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

/// Spawns a started server with a remote client's link and a host client's link.
fn spawn_links(app: &mut App) -> [Entity; 2] {
    let server = app.world_mut().spawn((Server::default(), Started)).id();
    let remote = app
        .world_mut()
        .spawn((
            LinkOf { server },
            ClientOf,
            RemoteId(PeerId::Local(1)),
            Connected,
        ))
        .id();
    let host = app
        .world_mut()
        .spawn((
            LinkOf { server },
            ClientOf,
            Client,
            RemoteId(PeerId::Local(0)),
            Connected,
        ))
        .id();
    [remote, host]
}

/// A remote client's link that disconnects, here as its link times out, is despawned at the end
/// of the frame, and its peer is unmapped; a host client's link is the client, and stays.
#[test]
fn disconnected_links_are_despawned_except_a_host_client() {
    let mut app = server_app();
    let [remote, host] = spawn_links(&mut app);

    app.world_mut().entity_mut(remote).insert(Unlinked {
        reason: UnlinkReason::TimedOut,
    });
    app.world_mut()
        .entity_mut(host)
        .insert(Disconnected::default());
    app.update();

    assert!(app.world().get_entity(remote).is_err());
    assert!(app.world().get_entity(host).is_ok());
    let ends = app.world().resource::<Ends>();
    assert_eq!(
        ends.disconnected,
        [
            (
                remote,
                DisconnectedReason::LinkFailed(UnlinkReason::TimedOut)
            ),
            (host, DisconnectedReason::Initial),
        ]
    );
    let peer_map = &app.world().resource::<NetworkingState>().peer_map;
    assert!(!peer_map.contains_key(&PeerId::Local(1)));
    assert!(!peer_map.contains_key(&PeerId::Local(0)));
}

/// A `Disconnect` on a server's link waits for the end of the frame, then reports its code as
/// requested, closes the link with it, and despawns the link. On a host client's link, the
/// server side does nothing: the host plugin handles the client.
#[test]
fn disconnect_on_a_server_link_ends_it_with_its_code() {
    let mut app = server_app();
    let [remote, host] = spawn_links(&mut app);
    let code = AppCode::new(AppCode::FIRST + 2);

    app.world_mut().trigger(Disconnect {
        entity: remote,
        reason: code,
    });
    app.world_mut().trigger(Disconnect::from(host));
    app.world_mut().flush();
    assert_eq!(
        app.world()
            .get::<Disconnecting>(remote)
            .map(|disconnecting| disconnecting.reason),
        Some(code)
    );
    assert!(!app.world().entity(host).contains::<Disconnecting>());

    app.update();

    assert!(app.world().get_entity(remote).is_err());
    assert!(app.world().entity(host).contains::<Connected>());
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
    let [old, _] = spawn_links(&mut app);
    let server = app.world().get::<LinkOf>(old).unwrap().server;
    let new = app
        .world_mut()
        .spawn((
            LinkOf { server },
            ClientOf,
            RemoteId(PeerId::Local(1)),
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
            .get(&PeerId::Local(1)),
        Some(&new)
    );
}

/// `peer_map` names the server while it is `Started`: going down already unmaps it.
#[test]
fn server_mapping_lasts_while_started() {
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
fn despawned_server_clears_peer_map() {
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
    let [remote, _] = spawn_links(&mut app);

    app.world_mut().entity_mut(remote).insert(Unlinked {
        reason: UnlinkReason::TimedOut,
    });
    app.update();

    assert!(app.world().get_entity(remote).is_err());
    assert!(
        !app.world()
            .resource::<NetworkingState>()
            .peer_map
            .contains_key(&PeerId::Local(1))
    );
}
