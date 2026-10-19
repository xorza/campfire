use bevy_ecs::component::Component;
use campfire_net_packets::ChannelBinding;

/// The binding of the TLS packet session a link's packet session came from, on the link's entity,
/// which an app's login signs. A plain packet session's link has none (13-net-stack/udp.md, The app
/// authenticates over the link).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkBinding(pub ChannelBinding);
