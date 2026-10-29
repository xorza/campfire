use core::net::SocketAddr;

use bevy_app::App;
use bevy_ecs::entity::Entity;
use campfire_net_emulator::EmulatedLinkId;

use crate::app_schedule::AppSchedule;

/// A remote client of the stepper: its app, its client entity there, the address its link has at
/// the server, the emulated link it is on, and how its app runs.
#[derive(Debug)]
pub(crate) struct RemoteClient {
    pub(crate) app: App,
    pub(crate) entity: Entity,
    pub(crate) address: SocketAddr,
    pub(crate) wire: EmulatedLinkId,
    pub(crate) schedule: AppSchedule,
}
