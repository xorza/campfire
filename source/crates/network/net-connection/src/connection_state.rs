use crate::networking_state::NetworkingState;
use crate::remote_id::RemoteId;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use bevy_reflect::Reflect;
use campfire_net_payload::AppCode;
use campfire_net_payload::UnlinkReason;

#[derive(Component, Debug, Reflect)]
#[component(on_add = Connected::on_add, on_remove = Connected::on_remove)]
pub struct Connected;

impl Connected {
    fn on_add(mut world: DeferredWorld<'_>, context: HookContext) {
        let peer_id = world
            .get::<RemoteId>(context.entity)
            .unwrap_or_else(|| {
                panic!(
                    "A Connected entity ({:?}) must always have a RemoteId component",
                    context.entity
                )
            })
            .0;
        world
            .commands()
            .entity(context.entity)
            .remove::<(Connecting, Disconnecting, Disconnected)>();
        if let Some(mut networking) = world.get_resource_mut::<NetworkingState>() {
            networking.peer_map.insert(peer_id, context.entity);
        }
    }

    fn on_remove(mut world: DeferredWorld<'_>, context: HookContext) {
        NetworkingState::unmap_remote(&mut world, context.entity);
    }
}

#[derive(Component, Default, Debug, Reflect)]
#[component(on_add = Connecting::on_add)]
pub struct Connecting;

impl Connecting {
    fn on_add(mut world: DeferredWorld<'_>, context: HookContext) {
        world
            .commands()
            .entity(context.entity)
            .remove::<(Connected, Disconnecting, Disconnected)>();
    }
}

#[derive(Component, Default, Debug, Reflect)]
#[component(on_add = Disconnected::on_add)]
pub struct Disconnected {
    /// Structured reason for the disconnection.
    pub reason: DisconnectedReason,
}

impl Disconnected {
    fn on_add(mut world: DeferredWorld<'_>, context: HookContext) {
        NetworkingState::unmap_remote(&mut world, context.entity);
        world
            .commands()
            .entity(context.entity)
            .remove::<(Connecting, Disconnecting, Connected)>();
    }
}

/// A connection that goes down at the end of the frame, after its last packets are sent.
#[derive(Component, Debug, Reflect)]
#[component(on_add = Disconnecting::on_add)]
pub struct Disconnecting {
    /// The code of the [`Disconnect`](crate::disconnect::Disconnect) that asked for it.
    pub reason: AppCode,
}

impl Disconnecting {
    fn on_add(mut world: DeferredWorld<'_>, context: HookContext) {
        world
            .commands()
            .entity(context.entity)
            .remove::<(Connected, Connecting, Disconnected)>();
    }
}

/// Why a [`Disconnected`] component was inserted on a connection entity.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum DisconnectedReason {
    /// The connection has not been attempted yet.
    #[default]
    Initial,
    /// This end requested the disconnect with its code.
    UserRequested(AppCode),
    /// The underlying link failed or its peer closed it.
    LinkFailed(UnlinkReason),
}

impl From<UnlinkReason> for DisconnectedReason {
    /// A link this end closed is a disconnect this end requested.
    fn from(reason: UnlinkReason) -> Self {
        match reason {
            UnlinkReason::Initial => Self::Initial,
            UnlinkReason::Closed(code) => Self::UserRequested(code),
            reason => Self::LinkFailed(reason),
        }
    }
}

impl core::fmt::Display for DisconnectedReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Initial => f.write_str("not connected yet"),
            Self::UserRequested(code) => write!(f, "requested with code {}", code.get()),
            Self::LinkFailed(reason) => write!(f, "link failed: {reason}"),
        }
    }
}
