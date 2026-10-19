use super::*;

use crate::connection_state::DisconnectedReason;

use campfire_net_payload::AppCode;

/// The client keeps the ids its IO layer set, and a `Disconnect` closes its link with its code,
/// which the disconnect reports as requested.
#[test]
fn raw_client_keeps_preconfigured_peer_ids_and_closes_with_its_code() {
    let mut app = App::new();
    app.add_plugins((campfire_net_link::LinkPlugin, RawClientPlugin));
    let local_id = LocalId(PeerId::Entity(1));
    let remote_id = RemoteId(PeerId::Entity(2));
    let entity = app
        .world_mut()
        .spawn((
            RawClient,
            LocalAddr("127.0.0.1:10001".parse().unwrap()),
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
            .get::<campfire_net_link::Unlinked>(entity)
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
