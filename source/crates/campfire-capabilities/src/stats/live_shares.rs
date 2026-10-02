use bevy_ecs::component::Component;

/// On a unit: it carries a live stat change, so its stats refresh every pass, as its sources'
/// stats change without its own state changing. Derived, never state.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LiveShares;
