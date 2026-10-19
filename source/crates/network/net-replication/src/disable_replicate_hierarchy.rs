use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use core::fmt::Debug;

/// When the `DisableReplicateHierarchy` marker component is added to an entity, we will stop
/// replicating their children.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct DisableReplicateHierarchy;
