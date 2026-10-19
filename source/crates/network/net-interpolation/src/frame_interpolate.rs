use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Marker component enabling frame interpolation on an entity.
///
/// The entity only needs this marker, and the stack selects the highest-priority matching
/// interpolation rule for each component or bundle on the entity's archetype.
///
/// # Examples
///
/// ```rust,ignore
/// use bevy_ecs::prelude::*;
/// use crate::frame_interpolate::FrameInterpolate;
///
/// fn spawn(mut commands: Commands) {
///     commands.spawn(FrameInterpolate);
/// }
/// ```
#[derive(Component, Default, PartialEq, Eq, Serialize, Deserialize, Clone, Debug, Reflect)]
pub struct FrameInterpolate;
