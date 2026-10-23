use bevy_ecs::component::Component;

/// A link whose player took a player slot, before or after the start, or whose spectator watches:
/// the server receives its inputs from then on.
#[derive(Component, Debug, Clone, Copy)]
pub struct Joined;
