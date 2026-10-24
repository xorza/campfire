use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// How deep a walker's walk is in a chain of steps aside: 0 for a walk of its own, the walker's
/// that it steps ahead of, and one more than the walker's that it steps to a side for; a walk
/// `MOST` deep clears no way, as Zero Hour's `moveAllies` stops past a depth of 2
/// (04-capabilities/navigation.md, An idle friend steps aside; walkers-get-through.md, A friend
/// yields through the friends that yield). The chain runs over ticks here, each step aside a walk
/// of its own tick, so its depth is a part.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AsideDepth(u8);

impl AsideDepth {
    /// The depth of a walk that clears no way.
    pub(crate) const MOST: u8 = 2;

    pub const fn get(self) -> u8 {
        self.0
    }

    /// Whether a walk of this depth clears its way of the units it may move.
    pub(crate) const fn clears(self) -> bool {
        self.0 < Self::MOST
    }

    /// The depth of a step aside for a walk of this depth.
    pub(crate) const fn deeper(self) -> Self {
        Self(self.0 + 1)
    }

    /// Sets the depth of the walk of `depth`.
    pub(crate) fn set(depth: &mut Mut<'_, Self>, value: Self) {
        depth.set_if_neq(value);
    }
}

impl SimComponent for AsideDepth {
    const NAME: &'static str = "navigation.aside_depth";
    type Host = UnitEntity;

    // No step aside goes past the deepest.
    fn check(&self, _: &World, _: Entity) -> bool {
        self.0 <= Self::MOST
    }
}

impl Replication for AsideDepth {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}
