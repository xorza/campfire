use bevy_ecs::component::Component;
use campfire_net_packets::ChannelBinding;

/// The binding of the TLS session a link's session came from, on the link's entity, which an
/// app's login signs. A plain session's link has none.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkBinding(pub ChannelBinding);
