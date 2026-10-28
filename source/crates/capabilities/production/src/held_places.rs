use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declarations::ById;
use campfire_sim::{SimComponent, StableId};
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::{ActionId, UnitType};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::producer_spec::ProducerSpec;
use crate::train_queue::TrainQueue;
use crate::train_spec::TrainSpec;

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

    /// Whether a train of `action` may join `queue`: one `trains` says is not at a place always,
    /// and one at a place while a place is free of a unit and of each train at a place the queue
    /// holds, as Zero Hour reserves a place as the train queues (defenses-and-airfields.md, A train
    /// at a place reserves it as it queues).
    pub fn admits(
        &self,
        action: ActionId,
        queue: &TrainQueue,
        trains: &ById<ActionId, TrainSpec>,
    ) -> bool {
        if !trains.get(action).is_some_and(|spec| spec.at_place) {
            return true;
        }
        let free = self.0.iter().filter(|held| held.is_none()).count();
        let queued = queue
            .entries()
            .iter()
            .filter(|queued| trains.get(queued.action).is_some_and(|spec| spec.at_place))
            .count();
        free > queued
    }

    /// Each place that holds a unit.
    pub(crate) fn held(&self) -> impl Iterator<Item = u8> {
        let held = self.0.iter().enumerate().filter(|(_, held)| held.is_some());
        held.map(|(at, _)| u8::try_from(at).expect("a producer's places fit u8"))
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

    /// Frees the place `place` of `unit`; whether it held it.
    pub(crate) fn release(&mut self, place: u8, unit: StableId) -> bool {
        let held = &mut self.0[usize::from(place)];
        let holds = *held == Some(unit);
        if holds {
            *held = None;
        }
        holds
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

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_sim::StableId;

    use crate::held_places::HeldPlaces;

    impl HeldPlaces {
        /// Places of `holders`, by place, each the unit it holds or free.
        pub fn of(holders: impl IntoIterator<Item = Option<StableId>>) -> Self {
            Self(holders.into_iter().collect())
        }
    }
}
