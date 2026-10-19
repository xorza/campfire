use crate::networking_state::NetworkingState;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use bevy_reflect::Reflect;
use campfire_net_core::PeerId;
use campfire_net_payload::AppCode;
use campfire_net_payload::UnlinkReason;
use core::fmt::Debug;
use tracing::trace;

/// A started server, which `peer_map` names as [`PeerId::Server`] while it lasts.
#[derive(Component, Debug, Reflect)]
#[component(on_add = Started::on_add, on_remove = Started::on_remove)]
pub struct Started;

impl Started {
    fn on_add(mut world: DeferredWorld<'_>, context: HookContext) {
        world
            .resource_mut::<NetworkingState>()
            .peer_map
            .insert(PeerId::Server, context.entity);
        trace!("Started added: removing Starting/Stopped/Stopping");
        world
            .commands()
            .entity(context.entity)
            .remove::<(Starting, Stopped, Stopping)>();
    }

    fn on_remove(mut world: DeferredWorld<'_>, context: HookContext) {
        if let Some(mut networking) = world.get_resource_mut::<NetworkingState>() {
            networking.unmap(PeerId::Server, context.entity);
        }
    }
}

#[derive(Component, Debug, Reflect)]
#[component(on_add = Starting::on_add)]
pub struct Starting;

impl Starting {
    fn on_add(mut world: DeferredWorld<'_>, context: HookContext) {
        trace!("Starting added: removing Started/Stopped/Stopping");
        world
            .commands()
            .entity(context.entity)
            .remove::<(Started, Stopped, Stopping)>();
    }
}

#[derive(Component, Default, Debug, Reflect)]
#[component(on_add = Stopped::on_add)]
pub struct Stopped {
    /// Structured reason for the stop.
    pub reason: StopReason,
}

impl Stopped {
    fn on_add(mut world: DeferredWorld<'_>, context: HookContext) {
        trace!("Stopped added: removing Started/Starting/Stopping");
        world
            .commands()
            .entity(context.entity)
            .remove::<(Started, Starting, Stopping)>();
    }
}

#[derive(Component, Debug, Reflect)]
#[component(on_add = Stopping::on_add)]
pub struct Stopping;

impl Stopping {
    fn on_add(mut world: DeferredWorld<'_>, context: HookContext) {
        trace!("Stopping added: removing Started/Starting/Stopped");
        world
            .commands()
            .entity(context.entity)
            .remove::<(Started, Starting, Stopped)>();
    }
}

/// Why a [`Stopped`] component was inserted on a server entity.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum StopReason {
    /// The server has not been started yet.
    #[default]
    Initial,
    /// This end requested the stop with its code.
    UserRequested(AppCode),
    /// The server's link failed.
    LinkFailed(UnlinkReason),
}

impl From<UnlinkReason> for StopReason {
    /// A link this end closed is a stop this end requested.
    fn from(reason: UnlinkReason) -> Self {
        match reason {
            UnlinkReason::Initial => Self::Initial,
            UnlinkReason::Closed(code) => Self::UserRequested(code),
            reason => Self::LinkFailed(reason),
        }
    }
}

impl core::fmt::Display for StopReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Initial => f.write_str("not started yet"),
            Self::UserRequested(code) => write!(f, "requested with code {}", code.get()),
            Self::LinkFailed(reason) => write!(f, "link failed: {reason}"),
        }
    }
}
