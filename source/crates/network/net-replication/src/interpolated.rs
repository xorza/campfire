use bevy_ecs::{component::Component, reflect::ReflectComponent};
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// A received entity the client shows at the interpolation timeline; it is the replication
/// crate's, as interpolation and replication both name it.
#[derive(Debug, Clone, Copy, Default, Reflect, Serialize, Deserialize, Component)]
#[reflect(Component)]
pub struct Interpolated;
