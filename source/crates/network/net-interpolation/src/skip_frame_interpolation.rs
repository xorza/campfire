use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// If present, this marker indicates that we will skip applying frame interpolation.
///
/// This can be useful for example if a character teleports and you don't want
/// to interpolate between the two positions.
///
/// You can add this directly on the client-side, or you can also add it on the
/// sender-side and replicate the component.
#[derive(Component, PartialEq, Eq, Serialize, Deserialize, Clone, Debug, Reflect)]
pub struct SkipFrameInterpolation;
