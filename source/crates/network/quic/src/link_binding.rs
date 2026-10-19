use bevy_ecs::component::Component;
use campfire_common::ChannelBinding;

/// The channel binding of the TLS session a link's entity carries, which its connect answer signs
/// (11-quic-transport.md, The answer binds to the TLS session): the IO puts it in a server's spawn
/// of the link and on a client's entity as it links; an in-process link takes one fixed binding at
/// both its ends.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkBinding(pub ChannelBinding);
