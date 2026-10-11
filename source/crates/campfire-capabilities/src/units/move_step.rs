use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentPredicted;
use crate::units::body::Body;

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
    const NAME: &'static str = "units.move_step";

    // Its decode keeps it at 0 or more; a unit that walks has a circle for a body, or none, as
    // a box never walks.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world
            .get::<Body>(entity)
            .is_none_or(|body| body.radius().is_some())
    }
}

impl Replication for MoveStep {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

/// A snapshot is untrusted, so a negative step fails to decode.
impl<'de> Deserialize<'de> for MoveStep {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<MoveStep, D::Error> {
        MoveStep::new(Num::deserialize(deserializer)?)
            .ok_or_else(|| D::Error::custom("negative move step"))
    }
}

#[cfg(test)]
mod tests {
    use campfire_common::Binary;

    use super::*;

    #[test]
    fn move_steps_are_never_negative() {
        assert_eq!(MoveStep::new(-Num::EPSILON), None);
        assert_eq!(MoveStep::new(Num::ZERO).map(MoveStep::get), Some(Num::ZERO));
        let negative = Binary::encode(&-Num::EPSILON);
        assert!(Binary::decode::<MoveStep>(&negative).is_err());
        let one = Binary::encode(&Num::ONE);
        assert_eq!(
            Binary::decode::<MoveStep>(&one).ok(),
            MoveStep::new(Num::ONE)
        );
    }
}
