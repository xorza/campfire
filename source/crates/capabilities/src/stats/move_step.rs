use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// How far a unit walks in one tick, never negative: the effect of its move speed, which its
/// stats give.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct MoveStep(Num);

impl MoveStep {
    /// `None` for a negative step.
    pub const fn new(step: Num) -> Option<MoveStep> {
        if step.to_bits() < 0 {
            return None;
        }
        Some(MoveStep(step))
    }

    pub const fn get(self) -> Num {
        self.0
    }
}

impl SimComponent for MoveStep {
    const NAME: &'static str = "navigation.move_step";

    // Its decode keeps it at 0 or more.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

/// A snapshot is untrusted, so a negative step fails to decode.
impl<'de> Deserialize<'de> for MoveStep {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<MoveStep, D::Error> {
        MoveStep::new(Num::deserialize(deserializer)?)
            .ok_or_else(|| D::Error::custom("negative move step"))
    }
}
