use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declarations::PerId;
use campfire_math::Num;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::move_step::MoveStep;
use crate::unit_entity::UnitEntity;

/// How much faster a walker may move each tick, in meters a tick, never negative: the effect of its
/// acceleration, which its stats give, for a movement model that keeps a velocity
/// (04-capabilities/navigation.md, A mover keeps a velocity).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct AccelStep(Num);

impl AccelStep {
    /// `None` for a negative step.
    pub const fn new(step: Num) -> Option<Self> {
        if step.to_bits() < 0 {
            return None;
        }
        Some(Self(step))
    }

    pub const fn get(self) -> Num {
        self.0
    }
}

impl SimComponent for AccelStep {
    const NAME: &'static str = "units.accel_step";
    type Host = UnitEntity;

    // Its decode keeps it at 0 or more; only a unit that walks speeds up.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world.get::<MoveStep>(entity).is_some()
    }
}

impl Replication for AccelStep {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

/// A snapshot is untrusted, so a negative step fails to decode.
impl<'de> Deserialize<'de> for AccelStep {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(Num::deserialize(deserializer)?)
            .ok_or_else(|| D::Error::custom("negative acceleration step"))
    }
}

impl PerId<UnitType> for AccelStep {
    const SCRIPTS_ONLY: bool = false;
}

#[cfg(test)]
mod tests {
    use campfire_codec::{Binary, BinaryError};

    use super::*;

    #[test]
    fn acceleration_steps_are_never_negative() {
        assert_eq!(AccelStep::new(-Num::EPSILON), None);
        assert_eq!(
            AccelStep::new(Num::ZERO).map(AccelStep::get),
            Some(Num::ZERO)
        );
        let negative = Binary::encode(&-Num::EPSILON);
        assert_eq!(
            Binary::decode::<AccelStep>(&negative).err(),
            Some(BinaryError::REFUSED)
        );
        let three = Binary::encode(&Num::int(3));
        assert_eq!(
            Binary::decode::<AccelStep>(&three).ok(),
            AccelStep::new(Num::int(3))
        );
    }
}
