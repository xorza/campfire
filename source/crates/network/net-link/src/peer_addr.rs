use bevy_ecs::prelude::*;
use core::net::SocketAddr;

/// The socket address of the peer that a link talks to.
///
/// An IO backend without OS sockets, such as one on WASM, does not insert it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct PeerAddr(pub SocketAddr);
