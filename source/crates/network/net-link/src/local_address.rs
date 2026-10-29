use core::net::SocketAddr;

use bevy_ecs::prelude::*;

/// The local socket address that a link sends from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct LocalAddress(pub SocketAddr);
