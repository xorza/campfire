use super::*;
use crate::connection_state::DisconnectedReason;
use campfire_net_payload::AppCode;
use campfire_net_payload::UnlinkReason;

/// The link's reason becomes the connection's: a link this end closed is a disconnect it
/// requested, a link that was never up is the initial state, and any other is a failure.
#[test]
fn link_failure_preserves_structured_reason() {
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
