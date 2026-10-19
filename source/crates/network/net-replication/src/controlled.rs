use crate::controlled_send::ControlledSend;
use crate::sent_marker::SentMarker;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Marker component indicating that the local peer controls this entity.
///
/// On the **client**, this is added to replicated entities that this client "owns" (e.g. the
/// player's own character). It is automatically inserted by the replication system when the server
/// sends a [`ControlledBy`](crate::controlled_by::ControlledBy) component pointing to this client's
/// link entity.
///
/// You can use `With<Controlled>` in queries to distinguish locally
/// controlled entities from remote ones.
#[derive(Component, Clone, PartialEq, Eq, Debug, Default, Reflect, Serialize, Deserialize)]
pub struct Controlled;

impl SentMarker for ControlledSend {
    type Received = Controlled;
}
