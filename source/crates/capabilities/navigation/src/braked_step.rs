use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// The step a walker was let take in the last Move stage, as its brake or its turn held it, none
/// when it took its whole step: a walker behind it brakes to that step, the advance it reads of
/// it, so the order the walkers move in changes none of them, and its own stuck count counts that
/// step while it is braked behind a unit that moves (entries-and-blocking.md, A walker brakes
/// behind a unit that blocks it).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BrakedStep(Option<Num>);

impl BrakedStep {
    pub const fn get(self) -> Option<Num> {
        self.0
    }

    /// Sets the step of `braked` to `step`.
    pub(crate) fn set(braked: &mut Mut<'_, Self>, step: Option<Num>) {
        braked.set_if_neq(Self(step));
    }
}

impl SimComponent for BrakedStep {
    const NAME: &'static str = "navigation.braked_step";
    type Host = UnitEntity;

    // A step a walker is let take is no negative.
    fn check(&self, _: &World, _: Entity) -> bool {
        self.0.is_none_or(|step| step >= Num::ZERO)
    }
}

impl Replication for BrakedStep {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}
