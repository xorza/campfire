use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Frame interpolation of the entity, by the highest-priority rule that matches each of its
/// components or bundles.
#[derive(Component, Default, PartialEq, Eq, Serialize, Deserialize, Clone, Debug, Reflect)]
pub struct FrameInterpolate;
