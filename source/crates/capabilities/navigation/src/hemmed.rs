use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// Whether a unit found no place to step aside to for a walker in its way: it is firm until it
/// walks, so routes price it as one no walker may move and the route a stuck walker asks walls
/// it (04-capabilities/navigation.md, A friend that cannot yield is firm, and a stuck walker's
/// route walls it). The search that finds no place runs in one tick, and the stuck walker asks
/// in a later one, so the mark is a part.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Hemmed(bool);

impl Hemmed {
    pub const fn get(self) -> bool {
        self.0
    }

    /// Marks the unit of `hemmed` as `value` says.
    pub(crate) fn set(hemmed: &mut Mut<'_, Self>, value: bool) {
        hemmed.set_if_neq(Self(value));
    }
}

impl SimComponent for Hemmed {
    const NAME: &'static str = "navigation.hemmed";
    type Host = UnitEntity;

    // Either value is a state a unit may be in.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Hemmed {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
