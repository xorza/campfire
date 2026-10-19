use bevy_ecs::prelude::*;
use campfire_net_core::NetTick;

/// A received entity whose server despawned it at [`Self::tick`], kept until the interpolation
/// timeline reaches that tick, as interpolation shows the past.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DelayedDespawn {
    tick: NetTick,
}

impl DelayedDespawn {
    pub(crate) const fn new(tick: NetTick) -> Self {
        Self { tick }
    }

    /// The server's tick of the despawn.
    pub const fn tick(&self) -> NetTick {
        self.tick
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::delayed_despawn::DelayedDespawn;
    use campfire_net_core::NetTick;

    impl DelayedDespawn {
        /// A despawn the server made at `tick`.
        pub const fn at(tick: NetTick) -> Self {
            Self::new(tick)
        }
    }
}
