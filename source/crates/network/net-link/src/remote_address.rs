use core::net::SocketAddr;

use bevy_ecs::prelude::*;

/// The socket address of the peer that a link talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct RemoteAddress(pub SocketAddr);
