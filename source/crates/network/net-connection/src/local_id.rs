use bevy_derive::Deref;
use bevy_ecs::component::Component;
use bevy_reflect::Reflect;
use campfire_net_core::PeerId;
use core::fmt::{Display, Formatter};
use serde::{Deserialize, Serialize};

/// Stores the PeerId of the local peer for the connection
#[derive(
    Debug, Component, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect, Deref,
)]
pub struct LocalId(pub PeerId);

impl Display for LocalId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.0, f)
    }
}
