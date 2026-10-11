use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentPredicted;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A unit's level, from 1: its type's stats grow with it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Level(u32);

impl Level {
    /// `None` for 0.
    pub const fn new(level: u32) -> Option<Level> {
        if level == 0 {
            return None;
        }
        Some(Level(level))
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Default for Level {
    fn default() -> Level {
        Level(1)
    }
}

impl SimComponent for Level {
    const NAME: &'static str = "stats.level";

    // Its decode keeps it at 1 or more, and a stat grows with any level.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Level {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

/// A snapshot is untrusted, so level 0 fails to decode.
impl<'de> Deserialize<'de> for Level {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Level, D::Error> {
        Level::new(u32::deserialize(deserializer)?)
            .ok_or_else(|| D::Error::custom("a level is at least 1"))
    }
}
