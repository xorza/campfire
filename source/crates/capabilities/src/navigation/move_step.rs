use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::Component;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::navigation::destination::Destination;
use crate::navigation::route::Route;

/// How far a unit walks in one tick, never negative.
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

    /// The components of a new unit that moves by this step, with nowhere to go yet.
    pub fn bundle(self) -> impl Bundle {
        (self, Destination::default(), Route::default())
    }
}

impl SimComponent for MoveStep {
    const NAME: &'static str = "navigation.move_step";
}

/// A snapshot is untrusted, so a negative step fails to decode.
impl<'de> Deserialize<'de> for MoveStep {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<MoveStep, D::Error> {
        MoveStep::new(Num::deserialize(deserializer)?)
            .ok_or_else(|| D::Error::custom("negative move step"))
    }
}
