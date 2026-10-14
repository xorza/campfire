use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::{DeferredWorld, World};
use campfire_sim::{Restoring, SimComponent};
use campfire_state_types::{DataKind, Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::UnitType;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::stat_book::StatBook;
use crate::unit_stats::UnitStats;

/// A unit's level, from 1: its type's stats grow with it. Its stats, which it requires, exist
/// exactly when it does, on every path, a spawn, a restore and a client's replication.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[component(on_add = Level::give_stats)]
#[require(UnitStats)]
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

    /// The hook that gives a unit its stats as its first refresh derives them, of its type at its
    /// level with no modifiers, so a reader in the stage it spawns in finds them. A restore puts
    /// untrusted values in, and a client may receive the level before the type: each keeps the
    /// empty stats, which the refresh after it derives, as it does those of every new unit.
    fn give_stats(mut world: DeferredWorld<'_>, context: HookContext) {
        if world.contains_resource::<Restoring>() {
            return;
        }
        let entity = context.entity;
        let (Some(book), Some(&unit_type)) = (
            world.get_resource::<StatBook>(),
            world.get::<UnitType>(entity),
        ) else {
            return;
        };
        let level = world
            .get::<Level>(entity)
            .expect("the hook's own component")
            .0;
        let stats = UnitStats::new(book.base_values(unit_type, level));
        *world
            .get_mut::<UnitStats>(entity)
            .expect("a level requires its stats") = stats;
    }
}

impl Default for Level {
    fn default() -> Level {
        Level(1)
    }
}

impl SimComponent for Level {
    const NAME: &'static str = "stats.level";
    type Host = UnitEntity;

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
