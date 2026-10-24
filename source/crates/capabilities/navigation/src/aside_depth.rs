use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// What a walker's walk is in a chain of steps aside: a walk of its own, 0 deep, or a step aside,
/// as deep as the walker's that it steps ahead of and one deeper than the walker's that it steps to
/// a side for, with where its unit stood as it began; a walk `MOST` deep clears no way, as Zero
/// Hour's `moveAllies` stops past a depth of 2 (04-capabilities/navigation.md, An idle friend steps
/// aside; 04-capabilities/navigation.md, A friend yields through the friends that yield). The chain
/// runs over ticks here, each step aside a walk of its own tick, so its depth is a part.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AsideDepth(Option<StepAside>);

/// A step aside: how deep it is, and where its unit stood as it began.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct StepAside {
    depth: u8,
    from: Vec3,
}

impl AsideDepth {
    /// The depth of a walk that clears no way.
    pub(crate) const MOST: u8 = 2;

    /// How deep the walk is: 0 for a walk of its own.
    pub const fn get(self) -> u8 {
        match self.0 {
            Some(step) => step.depth,
            None => 0,
        }
    }

    /// Whether a walk of this depth clears its way of the units it may move.
    pub(crate) const fn clears(self) -> bool {
        self.get() < Self::MOST
    }

    /// A step aside from `from` ahead of a walk of this depth.
    pub(crate) const fn ahead(self, from: Vec3) -> Self {
        Self::step(self.get(), from)
    }

    /// A step aside from `from` to a side for a walk of this depth.
    pub(crate) const fn deeper(self, from: Vec3) -> Self {
        Self::step(self.get() + 1, from)
    }

    const fn step(depth: u8, from: Vec3) -> Self {
        Self(Some(StepAside { depth, from }))
    }

    /// Whether a step aside whose unit stands at `at` went nowhere: within half its `step` of
    /// where it began.
    pub(crate) fn went_nowhere(self, at: Vec3, step: Num) -> bool {
        self.0
            .is_some_and(|aside| aside.from.within_ground(at, step / 2))
    }

    /// Sets the depth of the walk of `depth`.
    pub(crate) fn set(depth: &mut Mut<'_, Self>, value: Self) {
        depth.set_if_neq(value);
    }
}

impl SimComponent for AsideDepth {
    const NAME: &'static str = "navigation.aside_depth";
    type Host = UnitEntity;

    // No step aside goes past the deepest, and each began at a place within the world's bound.
    fn check(&self, _: &World, _: Entity) -> bool {
        self.0
            .is_none_or(|step| step.depth <= Self::MOST && step.from.within_bound())
    }
}

impl Replication for AsideDepth {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}
