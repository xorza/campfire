use bevy::state::state::States;

/// What the player's clicks do: select units and order them, place the build the command panel
/// started, which the build preview shows and takes the clicks of, or pick the point of a command
/// the panel started (02-engine-core.md, The input's modes are states).
#[derive(States, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum InputMode {
    #[default]
    Select,
    /// The build `Placing` holds, which goes as the mode leaves.
    Place,
    /// The command `Aiming` holds, which goes as the mode leaves.
    Aim,
}
