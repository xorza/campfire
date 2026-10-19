use std::sync::Weak;

use bevy_ecs::component::Component;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::observer::On;
use bevy_ecs::system::{Commands, Query};
use bevy_ecs::world::DeferredWorld;
use campfire_net_link::Unlink;
use campfire_net_packets::{CloseSession, PeerKey};
use campfire_net_payload::{AppCode, UnlinkReason};

/// The session a link entity carries. A local `Unlink` closes it with its code, and its removal
/// or replacement, by any path, closes it with `AppCode::ENDED`. It holds its endpoint weakly: the
/// endpoint alone owns the sessions, which its drop closes.
#[derive(Component, Debug)]
#[component(on_discard = Self::on_discard)]
pub(crate) struct SessionLink {
    endpoint: Weak<dyn CloseSession>,
    key: PeerKey,
}

impl SessionLink {
    pub(crate) fn new(endpoint: Weak<dyn CloseSession>, key: PeerKey) -> Self {
        Self { endpoint, key }
    }

    fn close(&self, code: AppCode) {
        if let Some(endpoint) = self.endpoint.upgrade() {
            endpoint.close(self.key, code);
        }
    }

    /// A local `Unlink` closes the session with its code, `ENDED` for any reason but a close, and
    /// the link parts from it.
    pub(crate) fn unlink(
        unlink: On<'_, '_, Unlink>,
        links: Query<'_, '_, &Self>,
        mut commands: Commands<'_, '_>,
    ) {
        let Ok(link) = links.get(unlink.entity) else {
            return;
        };
        let code = match unlink.reason {
            UnlinkReason::Closed(code) => code,
            _ => AppCode::ENDED,
        };
        link.close(code);
        commands.entity(unlink.entity).try_remove::<Self>();
    }

    fn on_discard(world: DeferredWorld<'_>, context: HookContext) {
        world
            .get::<Self>(context.entity)
            .expect("a hook of SessionLink runs on an entity that holds one")
            .close(AppCode::ENDED);
    }
}
