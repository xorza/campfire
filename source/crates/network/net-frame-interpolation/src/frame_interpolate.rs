use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Frame interpolation of the entity: each frame rule that writes on it blends its component's last
/// two fixed ticks into the visual; on an entity without it, the visual shows the live value.
#[derive(Component, Default, PartialEq, Eq, Serialize, Deserialize, Clone, Debug, Reflect)]
pub struct FrameInterpolate;
