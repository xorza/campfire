use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Sender-side marker that replicates as `Controlled` on the owning receiver.
///
/// This keeps `Controlled` receiver-local: a server's entity carries [`ControlledSend`] but not
/// `Controlled`.
#[derive(Component, Clone, PartialEq, Eq, Debug, Default, Reflect, Serialize, Deserialize)]
pub struct ControlledSend;
