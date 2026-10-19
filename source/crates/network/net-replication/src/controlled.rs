use crate::controlled_send::ControlledSend;
use crate::sent_marker::SentMarker;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// A received entity this client controls, as its server's
/// [`ControlledBy`](crate::controlled_by::ControlledBy) names this client's link.
#[derive(Component, Clone, PartialEq, Eq, Debug, Default, Reflect, Serialize, Deserialize)]
pub struct Controlled;

impl SentMarker for ControlledSend {
    type Received = Controlled;
}
