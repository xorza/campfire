use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;

/// Whether a walker leaves the producer that trained it: it passes through its team's units until
/// its body overlaps none, as Zero Hour's exit path may path through units
/// (04-capabilities/navigation.md, A new unit leaves its exit). The capability that trains it sets
/// it, and `navigation` clears it: navigation's state, kept here, as `Destination` is, so a
/// capability sets it with no dependency on navigation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Leaving(bool);

impl Leaving {
    pub const fn get(self) -> bool {
        self.0
    }

    /// Sets whether the walker of `leaving` leaves its producer.
    pub fn set(leaving: &mut Mut<'_, Self>, value: bool) {
        leaving.set_if_neq(Self(value));
    }
}

impl SimComponent for Leaving {
    const NAME: &'static str = "units.leaving";
    type Host = UnitEntity;

    // Either value holds on any unit that walks.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Leaving {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
