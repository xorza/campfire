use std::sync::Weak;
use std::time::Instant;

use bevy_ecs::component::Component;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::DeferredWorld;

use crate::app_code::AppCode;
use crate::peer::PeerKey;
use crate::shared_endpoint::SharedEndpoint;

/// The connection a link's entity carries: a `LinkOf` of a server, or a client's entity
/// (11-quic-transport.md, A link's entity is its peer, not its address). Its removal or replacement
/// closes the connection, whatever removes it: an `Unlink`, the server's end, which despawns every
/// `LinkOf` at once, or a despawn. It holds its endpoint weakly: the endpoint alone owns the
/// socket, which its drop closes with every connection.
#[derive(Component, Debug)]
#[component(on_discard = QuicLink::on_discard)]
pub(crate) struct QuicLink {
    endpoint: Weak<SharedEndpoint>,
    key: PeerKey,
}

impl QuicLink {
    pub(crate) const fn new(endpoint: Weak<SharedEndpoint>, key: PeerKey) -> Self {
        Self { endpoint, key }
    }

    /// Closes the connection with `code`, while its endpoint holds it.
    pub(crate) fn close(&self, code: AppCode) {
        let Some(endpoint) = self.endpoint.upgrade() else {
            return;
        };
        endpoint.lock().close(self.key, Instant::now(), code);
        endpoint.wake();
    }

    fn on_discard(world: DeferredWorld<'_>, context: HookContext) {
        world
            .get::<Self>(context.entity)
            .expect("a hook of QuicLink runs on an entity that holds one")
            .close(AppCode::ENDED);
    }
}
