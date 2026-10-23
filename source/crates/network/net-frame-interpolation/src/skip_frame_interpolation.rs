use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// The frame shows the newest fixed tick's value, with no blend from the tick before: as a
/// character teleports, its visual does not cross the gap.
///
/// You can add this directly on the client-side, or you can also add it on the
/// sender-side and replicate the component.
#[derive(Component, PartialEq, Eq, Serialize, Deserialize, Clone, Debug, Reflect)]
pub struct SkipFrameInterpolation;
