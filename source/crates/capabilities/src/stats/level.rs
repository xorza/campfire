use bevy_ecs::component::Component;
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
}

/// A snapshot is untrusted, so level 0 fails to decode.
impl<'de> Deserialize<'de> for Level {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Level, D::Error> {
        Level::new(u32::deserialize(deserializer)?)
            .ok_or_else(|| D::Error::custom("a level is at least 1"))
    }
}
