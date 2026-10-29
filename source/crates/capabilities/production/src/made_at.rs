use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{SimComponent, StableId};
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::door_times::DoorTimes;
use crate::doors::Doors;
use crate::held_places::HeldPlaces;

/// The producer and its place a unit was made at, which it holds while it lives
/// (04-capabilities/production.md, A jet holds a place).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MadeAt {
    pub producer: StableId,
    pub place: u8,
}

/// A unit made at a producer that is gone, taken in at a free place of another: the contract by
/// which an airfield takes in a unit of a gone one (04-capabilities/airfields.md, A jet goes home).
#[derive(Debug)]
pub struct Rehome<'a, 'w> {
    /// The unit and where it was made.
    pub unit: StableId,
    pub made: &'a mut Mut<'w, MadeAt>,
    /// The producer that takes it in, its free place, and that producer's held places.
    pub producer: StableId,
    pub place: u8,
    pub held: &'a mut HeldPlaces,
    /// The producer's doors and their times, when it has doors.
    pub doors: Option<(&'a mut Doors, DoorTimes)>,
    pub now: Tick,
}

impl Rehome<'_, '_> {
    /// Holds the place for the unit, holds its door open, as Zero Hour's `reserveSpace` holds it
    /// open, and makes the unit the new producer's.
    pub fn apply(self) {
        self.held.hold(self.place, self.unit);
        if let Some((doors, times)) = self.doors {
            doors.hold(self.place, self.now, times);
        }
        self.made.set_if_neq(MadeAt {
            producer: self.producer,
            place: self.place,
        });
    }
}

impl SimComponent for MadeAt {
    const NAME: &'static str = "production.made_at";
    type Host = UnitEntity;

    // Any producer and place: the producer may be gone.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for MadeAt {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
