//! Verifies that the protocols of the client and server match.

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use campfire_net_connection::ClientOf;
use campfire_net_connection::Connected;
use campfire_net_core::LinkDirection;
use campfire_net_core::RegistryHash;
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
    messages: Option<RegistryHash>,
    components: Option<RegistryHash>,
    channels: Option<RegistryHash>,
}

/// This app's own checksums, frozen once its plugins finish registering.
#[derive(Resource, Debug, Clone, Copy)]
struct ProtocolHashes(ProtocolCheck);

/// The server sends each client that connects its registries' hashes, and a client whose own differ
/// unlinks (11-net-stack/transport.md, Both peers agree on the protocol before data flows).
#[derive(Debug)]
pub(crate) struct ProtocolCheckPlugin;

impl ProtocolCheckPlugin {
    /// On the server, send a message with the protocol checksum when a client connects
    fn send_verify_protocol(
        trigger: On<'_, '_, Add<Connected>>,
        mut sender: Query<'_, '_, &mut EventSender<ProtocolCheck>, With<ClientOf>>,
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
            .add_direction(LinkDirection::ServerToClient);
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
mod tests;
