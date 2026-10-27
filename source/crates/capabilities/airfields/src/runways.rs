use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declarations::ById;
use campfire_sim::{SimComponent, StableId};
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::airfield_data::AirfieldData;

/// The unit each of an airfield's runways holds, by runway: one at a time, as it takes off or
/// lands (jets.md, A jet takes off and lands on its runway).
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Runways(Box<[Option<StableId>]>);

impl Runways {
    /// `count` runways, each free.
    pub(crate) fn free_of(count: usize) -> Self {
        Self(vec![None; count].into())
    }

    /// The unit the runway `runway` holds.
    pub fn holder(&self, runway: u8) -> Option<StableId> {
        self.0[usize::from(runway)]
    }

    /// Holds the runway `runway` for `unit`, which it held already or which is free: whether it
    /// holds it now.
    pub(crate) fn take(&mut self, runway: u8, unit: StableId) -> bool {
        let held = &mut self.0[usize::from(runway)];
        if held.is_some_and(|holder| holder != unit) {
            return false;
        }
        *held = Some(unit);
        true
    }

    /// Frees the runway `runway` of `unit`, which held it.
    pub(crate) fn release(&mut self, runway: u8, unit: StableId) {
        let held = &mut self.0[usize::from(runway)];
        if *held == Some(unit) {
            *held = None;
        }
    }
}

impl SimComponent for Runways {
    const NAME: &'static str = "airfields.runways";
    type Host = UnitEntity;

    // As many runways as its type's airfield names.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.resource::<ById<UnitType, AirfieldData>>();
        world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| book.get(unit_type))
            .is_some_and(|airfield| airfield.runways.len() == self.0.len())
    }
}

impl Replication for Runways {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
