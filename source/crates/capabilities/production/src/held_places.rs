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

use crate::producer_spec::ProducerSpec;

/// The unit each of a producer's places holds, by place: the unit its train made there, while it
/// lives (04-capabilities/production.md, A jet holds a place).
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HeldPlaces(Box<[Option<StableId>]>);

impl HeldPlaces {
    /// `count` places, each free.
    pub(crate) fn free_of(count: usize) -> Self {
        Self(vec![None; count].into())
    }

    /// The first free place, none while each holds a unit.
    pub fn free(&self) -> Option<u8> {
        let at = self.0.iter().position(Option::is_none)?;
        Some(u8::try_from(at).expect("a producer's places fit u8"))
    }

    /// The unit the place `place` holds.
    pub fn holder(&self, place: u8) -> Option<StableId> {
        self.0.get(usize::from(place)).copied().flatten()
    }

    /// Holds the free place `place` for `unit`, a unit of another producer that lost its own: the
    /// contract by which an airfield takes in a unit of a gone one (04-capabilities/airfields.md, A
    /// jet goes home).
    pub fn hold_for(&mut self, place: u8, unit: StableId) {
        self.hold(place, unit);
    }

    /// Holds the free place `place` for `unit`.
    pub(crate) fn hold(&mut self, place: u8, unit: StableId) {
        let held = &mut self.0[usize::from(place)];
        debug_assert!(held.is_none(), "a train is made at a free place");
        *held = Some(unit);
    }

    /// Frees the place `place` of `unit`, which it held.
    pub(crate) fn release(&mut self, place: u8, unit: StableId) {
        let held = &mut self.0[usize::from(place)];
        if *held == Some(unit) {
            *held = None;
        }
    }
}

impl SimComponent for HeldPlaces {
    const NAME: &'static str = "production.held_places";
    type Host = UnitEntity;

    // As many places as its type's production names.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let specs = world.resource::<ById<UnitType, ProducerSpec>>();
        world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| specs.get(unit_type))
            .is_some_and(|spec| spec.places.len() == self.0.len())
    }
}

impl Replication for HeldPlaces {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
