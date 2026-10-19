use bevy_ecs::{component::Component, reflect::ReflectComponent};
use bevy_reflect::Reflect;

/// A server's link to one client, from the frame it is first
/// [`Connected`](crate::connection_state::Connected) until the connection layer despawns it: it
/// stays while the link disconnects.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Reflect)]
#[reflect(Component, PartialEq, Debug, Clone)]
pub struct ClientOf;
