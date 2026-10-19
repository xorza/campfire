use bevy_ecs::prelude::*;

/// When a receiver despawns an interpolated entity that its server despawned.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InterpolatedDespawn {
    /// At once, as any other entity.
    #[default]
    Immediate,
    /// When the interpolation timeline reaches the server's tick of the despawn, as
    /// [`DelayedDespawn`](crate::delayed_despawn::DelayedDespawn): interpolation sets this.
    AtInterpolationTick,
}
