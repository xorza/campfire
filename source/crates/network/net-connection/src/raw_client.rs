//! A connection whose link is the connection: linking connects, and the [`LocalId`] and
//! [`RemoteId`] come from the link's addresses unless the IO layer sets them.
//!
//! [`LocalId`]: crate::local_id::LocalId
//! [`RemoteId`]: crate::remote_id::RemoteId

use crate::client::Client;
use crate::connection_state::Disconnected;
use bevy_ecs::prelude::*;
use campfire_net_link::Link;

/// Marker type to represent a client whose IO layer also acts as its Connection layer
///
/// In this case, Linked/Connected are equivalent; same for Unlinked/Disconnected.
///
/// The PeerId associated with the connection is the entity itself.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(Link, Client)]
#[require(Disconnected)]
pub struct RawClient;
