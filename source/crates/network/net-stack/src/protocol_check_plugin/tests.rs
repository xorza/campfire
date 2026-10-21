use super::*;
use campfire_net_core::PeerId;
use campfire_net_messages::MessagePlugin;

#[derive(Resource, Default)]
struct Unlinks(Vec<(Entity, UnlinkReason)>);

/// An app with the protocol check and the message layer that seals its registries, whose
/// server link is `link`.
fn client_app() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((MessagePlugin, ProtocolCheckPlugin));
    app.finish();
    app.init_resource::<Unlinks>();
    app.add_observer(
        |unlink: On<'_, '_, Unlink>, mut unlinks: ResMut<'_, Unlinks>| {
            unlinks.0.push((unlink.entity, unlink.reason));
        },
    );
    let link = app.world_mut().spawn_empty().id();
    (app, link)
}

/// The server's own hashes pass; any one registry that differs unlinks the link the check
/// arrived on.
#[test]
fn a_differing_protocol_unlinks() {
    let (mut app, link) = client_app();
    let local = app.world().resource::<ProtocolHashes>().0;
    let zero = Some(RegistryHash::new([0; 32]));
    let differing = [
        local,
        ProtocolCheck {
            messages: local.messages.map(|hash| {
                let mut bytes = hash.get();
                bytes[0] ^= 1;
                RegistryHash::new(bytes)
            }),
            ..local
        },
        ProtocolCheck {
            components: zero,
            ..local
        },
        ProtocolCheck {
            channels: zero,
            ..local
        },
    ];
    for (index, remote) in differing.into_iter().enumerate() {
        app.world_mut().resource_mut::<Unlinks>().0.clear();
        app.world_mut().trigger(RemoteEvent {
            event: remote,
            from: PeerId::Server,
            link,
        });
        app.world_mut().flush();
        let expected = if index == 0 {
            vec![]
        } else {
            vec![(link, UnlinkReason::ProtocolMismatch)]
        };
        assert_eq!(app.world().resource::<Unlinks>().0, expected, "{remote:?}");
    }
}
