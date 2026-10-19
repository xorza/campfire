use bevy::ecs::resource::Resource;
use campfire_math::Num;
use campfire_sim::StableId;
use campfire_values::{ActionId, UnitType};

/// The build the player places, from a build button of the command panel, while the build
/// preview shows it: the builder, its slot of the build, the build's action, the building's type,
/// and its angle in degrees. It exists while the input's mode is `InputMode::Place`.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placing {
    pub(crate) builder: StableId,
    pub(crate) slot: u8,
    pub(crate) action: ActionId,
    pub(crate) building: UnitType,
    pub(crate) angle: Num,
}
