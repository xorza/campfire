use bevy_ecs::prelude::Resource;
use bevy_ecs::system::Res;
use campfire_net_core::NetTick;

/// Application-global resource present while the simulation is being rolled back: the tick the
/// predicted state is restored to, from which the replay runs each tick to the present.
///
/// It lives in the timeline crate, which each layer that behaves differently during a rollback
/// already depends on.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rollback(pub NetTick);

impl Rollback {
    /// The run condition that the app is rolling back.
    pub const fn is_active(rollback: Option<Res<'_, Self>>) -> bool {
        rollback.is_some()
    }
}
