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

/// The place on the ground a winged flier holds, which it circles in the air with no goal: where
/// it spawned, then the goal it last arrived at (jets.md, A jet flies on wings).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HeldPlace(Vec3);

impl HeldPlace {
    /// The place of `at` on the ground.
    pub const fn new(at: Vec3) -> Self {
        Self(at.ground())
    }

    pub const fn get(self) -> Vec3 {
        self.0
    }
}

impl SimComponent for HeldPlace {
    const NAME: &'static str = "navigation.held_place";
    type Host = UnitEntity;

    // A place on the ground within the world's bound, of a unit whose type flies on wings.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.get_resource::<ById<UnitType, FlightData>>();
        let winged = world.get::<UnitType>(entity).is_some_and(|&unit_type| {
            book.and_then(|book| book.get(unit_type))
                .is_some_and(|flight| flight.wings.is_some())
        });
        winged && self.0.y == Num::ZERO && self.0.within_bound()
    }
}

impl Replication for HeldPlace {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
