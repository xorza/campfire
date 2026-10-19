use bevy_ecs::prelude::*;
use core::net::SocketAddr;

/// The local socket address that a link sends from.
///
/// An IO backend without OS sockets, such as one on WASM, does not insert it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct LocalAddr(pub SocketAddr);
