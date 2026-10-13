use bevy_ecs::component::Component;

/// Marks a unit whose type's tags hold the engine's `avatar`, the unit a player plays as, so a
/// lookup of a player's avatar visits the avatars alone. Derived from its type, never state.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct Avatar;
