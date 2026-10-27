use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;

/// Where a flier's flight takes it: up to its type's height, or down to the ground, where the
/// capability that parks it holds it. State of `navigation`'s fliers, kept here, as `Destination`
/// is, for the capabilities above navigation to write (04-capabilities/airfields.md, A jet takes
/// off and lands on its runway).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlightGoal {
    #[default]
    Height,
    Ground,
}

impl SimComponent for FlightGoal {
    const NAME: &'static str = "units.flight_goal";
    type Host = UnitEntity;

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for FlightGoal {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
