use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::NotSent;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// How far a unit sees, in meters on the ground plane: it reveals every grid cell whose center
/// is that close, while it lives.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Sight(Num);

impl Sight {
    /// `None` for a negative range.
    pub(crate) const fn new(range: Num) -> Option<Sight> {
        if range.to_bits() < 0 {
            return None;
        }
        Some(Sight(range))
    }

    pub(crate) const fn range(self) -> Num {
        self.0
    }
}

impl SimComponent for Sight {
    const NAME: &'static str = "vision.sight";

    // Its decode keeps it at 0 or more.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Sight {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

/// A snapshot is untrusted, so a negative range fails to decode.
impl<'de> Deserialize<'de> for Sight {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Sight, D::Error> {
        Sight::new(Num::deserialize(deserializer)?)
            .ok_or_else(|| D::Error::custom("a negative sight range"))
    }
}
