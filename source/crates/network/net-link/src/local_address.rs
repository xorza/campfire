use bevy_ecs::prelude::*;
use core::net::SocketAddr;

/// The local socket address that a link sends from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct LocalAddress(pub SocketAddr);
