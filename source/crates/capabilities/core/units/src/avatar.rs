use bevy_ecs::component::Component;
use campfire_script::UnitRow;
use campfire_values::{EngineTag, TagProperties};

/// Marks a unit whose type's tags hold the engine's `avatar`, the unit a player plays as, so a
/// lookup of a player's avatar visits the avatars alone. Derived from its type, never state.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct Avatar;

impl Avatar {
    /// The engine tag of avatars, the units players play as, which `unit.is_avatar` tests.
    pub const TAG: EngineTag = EngineTag::new(0, "avatar", TagProperties::NONE);

    /// Whether the unit of `row` is tagged an avatar.
    pub(crate) const fn is_tagged(row: &UnitRow) -> bool {
        row.tags.tags.contains(Self::TAG.tag())
    }
}
