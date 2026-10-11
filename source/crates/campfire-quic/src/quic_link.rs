use std::sync::Weak;
use std::time::Instant;

use bevy_ecs::component::Component;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::DeferredWorld;
use quinn_proto::ConnectionHandle;

use crate::endpoint_state::CLOSED;
use crate::shared_endpoint::SharedEndpoint;

/// The connection a link's entity carries: a `LinkOf` of a server, or a client's entity (design
/// 15, D9). Its removal or replacement closes the connection, whatever removes it: an `Unlink`,
/// the server's end, which despawns every `LinkOf` at once, or a despawn. It holds its endpoint
/// weakly: the endpoint alone owns the socket, which its drop closes with every connection.
#[derive(Component, Debug)]
#[component(on_discard = QuicLink::on_discard)]
pub(crate) struct QuicLink {
    endpoint: Weak<SharedEndpoint>,
    handle: ConnectionHandle,
    id: u64,
}

impl QuicLink {
    pub(crate) const fn new(
        endpoint: Weak<SharedEndpoint>,
        handle: ConnectionHandle,
        id: u64,
    ) -> QuicLink {
        QuicLink {
            endpoint,
            handle,
            id,
        }
    }

    fn on_discard(world: DeferredWorld<'_>, context: HookContext) {
        let link = world
            .get::<QuicLink>(context.entity)
            .expect("a hook of QuicLink runs on an entity that holds one");
        let Some(endpoint) = link.endpoint.upgrade() else {
            return;
        };
        if let Some(peer) = endpoint.lock().peer(link.handle, link.id) {
            peer.close(Instant::now(), context.entity, CLOSED);
        }
        endpoint.wake();
    }
}
