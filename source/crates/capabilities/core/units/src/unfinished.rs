use bevy_ecs::component::Component;

/// A unit not finished yet, as a building its builders have yet to complete: the capability that
/// builds it marks it, so what pays or counts a unit by whether it is finished reads this, not
/// that capability's state (04-capabilities/games.md, Income over time). Derived, not state: the
/// builder's own state requires it, and its completion removes it.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub struct Unfinished;
