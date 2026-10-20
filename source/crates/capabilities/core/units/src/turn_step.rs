use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declarations::PerType;
use campfire_math::Num;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_visibility::DataKind;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::move_step::MoveStep;
use crate::unit_entity::UnitEntity;

/// How far a walker turns in one tick, in degrees, never negative: the effect of its turn rate,
/// which its stats give (04-capabilities/navigation.md, A walker faces its leg and turns at its
/// turn rate). A walker of none turns at once.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct TurnStep(Num);

impl TurnStep {
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

impl SimComponent for TurnStep {
    const NAME: &'static str = "units.turn_step";
    type Host = UnitEntity;

    // Its decode keeps it at 0 or more; only a unit that walks turns.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world.get::<MoveStep>(entity).is_some()
    }
}

impl Replication for TurnStep {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

/// A snapshot is untrusted, so a negative step fails to decode.
impl<'de> Deserialize<'de> for TurnStep {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(Num::deserialize(deserializer)?)
            .ok_or_else(|| D::Error::custom("negative turn step"))
    }
}

impl PerType for TurnStep {
    const SCRIPTS_ONLY: bool = false;
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;

    #[test]
    fn turn_steps_are_never_negative() {
        assert_eq!(TurnStep::new(-Num::EPSILON), None);
        assert_eq!(TurnStep::new(Num::ZERO).map(TurnStep::get), Some(Num::ZERO));
        let negative = Binary::encode(&-Num::EPSILON);
        assert!(Binary::decode::<TurnStep>(&negative).is_err());
        let three = Binary::encode(&Num::int(3));
        assert_eq!(
            Binary::decode::<TurnStep>(&three).ok(),
            TurnStep::new(Num::int(3))
        );
    }
}
