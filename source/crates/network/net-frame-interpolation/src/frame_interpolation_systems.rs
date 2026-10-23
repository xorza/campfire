use bevy_ecs::prelude::*;

/// The sets of `FrameInterpolationPlugin`: the frame histories record each fixed tick, and each
/// frame then writes the visual values from them.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum FrameInterpolationSystems {
    /// Records each frame-interpolated component's value at the fixed tick that just ran.
    Update,
    /// Writes each visual value the frame shows, one tick late, at the fixed overstep.
    Interpolate,
}
