use core::fmt::{Display, Formatter};

use bevy_derive::Deref;
use bevy_ecs::component::Component;
use bevy_reflect::Reflect;
use campfire_net_core::PeerId;
use serde::{Deserialize, Serialize};

/// Stores the PeerId of the remote peer that we are connected to
#[derive(
    Debug, Component, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect, Deref,
)]
#[component(immutable)]
pub struct RemoteId(pub PeerId);

impl Display for RemoteId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.0, f)
    }
}
