use bevy_ecs::component::Component;

/// A link whose player took a seat, before or after the start, or whose spectator watches: the
/// server receives its inputs from then on.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct Joined;
