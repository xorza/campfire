use super::*;
use crate::connection_state::DisconnectedReason;
use campfire_net_payload::AppCode;

/// The link's reason becomes the connection's: a link this end closed is a disconnect it
/// requested, a link that was never up is the initial state, and any other is a failure.
#[test]
fn a_link_failure_keeps_its_reason() {
    let code = AppCode::new(AppCode::FIRST);
    let cases = [
        (
            UnlinkReason::ClosedByPeer(code),
            DisconnectedReason::LinkFailed(UnlinkReason::ClosedByPeer(code)),
        ),
        (
            UnlinkReason::Closed(code),
            DisconnectedReason::UserRequested(code),
        ),
        (UnlinkReason::Initial, DisconnectedReason::Initial),
    ];
    for (unlink, disconnected) in cases {
        let mut app = App::new();
        app.add_plugins(ClientConnectionPlugin);
        let entity = app.world_mut().spawn(Unlinked { reason: unlink }).id();

        app.update();

        assert_eq!(
            app.world().get::<Disconnected>(entity).unwrap().reason,
            disconnected,
            "{unlink:?}"
        );
    }
}

/// A client starts disconnected unless it spawns in another state.
#[test]
fn a_client_starts_disconnected_unless_it_spawns_connected() {
    let mut app = App::new();
    app.add_plugins(ClientConnectionPlugin);
    let fresh = app.world_mut().spawn(Client).id();
    let connected = app
        .world_mut()
        .spawn((Client, RemoteId(PeerId::Entity(0)), Connected))
        .id();
    app.world_mut().flush();

    assert_eq!(
        app.world()
            .get::<Disconnected>(fresh)
            .map(|disconnected| disconnected.reason),
        Some(DisconnectedReason::Initial)
    );
    let connected = app.world().entity(connected);
    assert!(connected.contains::<Connected>());
    assert!(!connected.contains::<Disconnected>());
}

/// The client keeps the ids its IO layer set, and a `Disconnect` closes its link with its code,
/// which the disconnect reports as requested.
#[test]
fn a_client_keeps_the_ids_its_io_layer_set_and_a_disconnect_closes_its_link_with_its_code() {
    let mut app = App::new();
    app.add_plugins((campfire_net_link::LinkPlugin, ClientConnectionPlugin));
    let local_id = LocalId(PeerId::Entity(1));
    let remote_id = RemoteId(PeerId::Entity(2));
    let entity = app
        .world_mut()
        .spawn((
            Client,
            LocalAddress("127.0.0.1:10001".parse().unwrap()),
            local_id,
            remote_id,
        ))
        .id();

    app.world_mut().entity_mut(entity).insert(Linked);
    app.world_mut().flush();

    let linked = app.world().entity(entity);
    assert!(linked.contains::<Connected>());
    assert_eq!(linked.get::<LocalId>(), Some(&local_id));
    assert_eq!(linked.get::<RemoteId>(), Some(&remote_id));

    let code = AppCode::new(AppCode::FIRST + 1);
    app.world_mut().trigger(Disconnect {
        entity,
        reason: code,
    });
    app.world_mut().flush();
    assert_eq!(
        app.world()
            .get::<Unlinked>(entity)
            .map(|unlinked| unlinked.reason),
        Some(UnlinkReason::Closed(code))
    );
    assert_eq!(
        app.world()
            .get::<Disconnected>(entity)
            .map(|disconnected| disconnected.reason),
        Some(DisconnectedReason::UserRequested(code))
    );
}
