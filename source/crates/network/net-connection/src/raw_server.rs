use crate::server_state::Stopped;
use bevy_ecs::prelude::*;
use campfire_net_link::Server;
use core::fmt::Debug;

/// Marker type to represent a server whose IO layer also acts as its Connection layer
///
/// In this case, Linked/Started are equivalent; same for Unlinked/Stopped.
///
/// The PeerId associated with the connection is the entity itself.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(Server, Stopped)]
pub struct RawServer;
