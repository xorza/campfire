use bevy_ecs::prelude::*;

/// System sets used by `FrameInterpolationPlugin`.
///
/// These sets help order the systems responsible for:
/// - Restoring the actual component values before other game logic.
/// - Updating the history of component values used for interpolation.
/// - Performing the visual interpolation itself.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum FrameInterpolationSystems {
    /// Restore the correct component values when we start the FixedMainLoop.
    Restore,
    /// Update the previous/current component values used for visual interpolation.
    Update,
    /// Interpolate the visual state of the game with 1 tick of delay.
    Interpolate,
}
