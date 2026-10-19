//! Verifies that the protocols of the client and server match.

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use campfire_net_connection::ClientOf;
use campfire_net_connection::Connected;
use campfire_net_connection::HostClient;
use campfire_net_core::Direction;
use campfire_net_link::Unlink;
use campfire_net_messages::MessageRegistry;
use campfire_net_messages::{AppEventExt, EventSender, RemoteEvent};
use campfire_net_payload::UnlinkReason;
use campfire_net_replication::ComponentRegistry;
use campfire_net_replication::HelloChannel;
use campfire_net_transport::ChannelRegistry;
use serde::{Deserialize, Serialize};
use tracing::{trace, warn};

/// The protocol checksums the server sends to each client that connects.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Default, Clone, Copy, Event)]
pub(crate) struct ProtocolCheck {
    messages: Option<u64>,
    components: Option<u64>,
    channels: Option<u64>,
}

/// This app's own checksums, frozen once its plugins finish registering.
#[derive(Resource, Debug, Clone, Copy)]
struct ProtocolHashes(ProtocolCheck);

#[derive(Debug)]
pub(crate) struct ProtocolCheckPlugin;

impl ProtocolCheckPlugin {
    /// On the server, send a message with the protocol checksum when a client connects
    fn send_verify_protocol(
        trigger: On<'_, '_, Add<Connected>>,
        mut sender: Query<
            '_,
            '_,
            &mut EventSender<ProtocolCheck>,
            (With<ClientOf>, Without<HostClient>),
        >,
        hashes: Res<'_, ProtocolHashes>,
    ) {
        if let Ok(mut s) = sender.get_mut(trigger.entity) {
            s.trigger::<HelloChannel>(hashes.0);
        }
    }

    /// On the client, unlink from a server whose protocol differs.
    fn receive_verify_protocol(
        trigger: On<'_, '_, RemoteEvent<ProtocolCheck>>,
        hashes: Res<'_, ProtocolHashes>,
        mut commands: Commands<'_, '_>,
    ) {
        let remote = trigger.event;
        trace!("Received protocol check from server: {remote:?}");
        if remote == hashes.0 {
            return;
        }
        let local = hashes.0;
        warn!(
            messages_match = remote.messages == local.messages,
            components_match = remote.components == local.components,
            channels_match = remote.channels == local.channels,
            "unlinking from a server whose protocol differs"
        );
        commands.trigger(Unlink {
            entity: trigger.link,
            reason: UnlinkReason::ProtocolMismatch,
        });
    }
}

impl Plugin for ProtocolCheckPlugin {
    fn build(&self, app: &mut App) {
        app.register_event::<ProtocolCheck>()
            .add_direction(Direction::ServerToClient);
    }

    /// Reads each registry's hash, which its own plugin's `finish` sealed before this one runs.
    fn finish(&self, app: &mut App) {
        let world = app.world();
        let sealed = "a registry is sealed by its plugin, which finishes before the protocol check";
        let hashes = ProtocolCheck {
            messages: world
                .get_resource::<MessageRegistry>()
                .map(|registry| registry.hash().expect(sealed)),
            components: world
                .get_resource::<ComponentRegistry>()
                .map(|registry| registry.hash().expect(sealed)),
            channels: world
                .get_resource::<ChannelRegistry>()
                .map(|registry| registry.hash().expect(sealed)),
        };
        app.insert_resource(ProtocolHashes(hashes));

        app.add_observer(Self::send_verify_protocol);
        app.add_observer(Self::receive_verify_protocol);
    }
}

#[cfg(test)]
mod tests {
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
        let differing = [
            local,
            ProtocolCheck {
                messages: local.messages.map(|hash| hash ^ 1),
                ..local
            },
            ProtocolCheck {
                components: Some(0),
                ..local
            },
            ProtocolCheck {
                channels: Some(0),
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
}
