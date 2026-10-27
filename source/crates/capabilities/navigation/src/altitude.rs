use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declarations::ById;
use campfire_math::{Num, Vec3};
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::flight_data::FlightData;

/// A flier's height in the world as it last flew, which its next flight moves toward the ground
/// under it plus its type's height, so a step of the ground under it changes its height above the
/// ground until it climbs or sinks back (04-capabilities/navigation.md, A flier holds a height).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Altitude(Num);

impl Altitude {
    pub const fn new(height: Num) -> Self {
        Self(height)
    }

    pub const fn get(self) -> Num {
        self.0
    }
}

impl SimComponent for Altitude {
    const NAME: &'static str = "navigation.altitude";
    type Host = UnitEntity;

    // A height lies within the world's bound, as every place's does, of a unit whose type flies.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.get_resource::<ById<UnitType, FlightData>>();
        let flies = world
            .get::<UnitType>(entity)
            .is_some_and(|&unit_type| book.is_some_and(|book| book.get(unit_type).is_some()));
        flies && Vec3::new(Num::ZERO, self.0, Num::ZERO).within_bound()
    }
}

impl Replication for Altitude {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
