use bevy_ecs::prelude::*;
use core::hash::Hash;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum PreSpawnedSystems {
    /// In `PostUpdate`: despawns the pre-spawned entities that no server entity matched in time.
    CleanUp,
}
