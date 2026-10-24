use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Vec3;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// The goal a walker last arrived at, at its end or short of it, while it stands; none once it
/// takes another destination. A walker bound for that goal stops against it, as one of the crowd
/// there, and steps it not aside (04-capabilities/navigation.md, A crowded goal ends the walk).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArrivedAt(Option<Vec3>);

impl ArrivedAt {
    pub const fn get(self) -> Option<Vec3> {
        self.0
    }

    /// Sets the goal the walker of `arrived` last arrived at.
    pub(crate) fn set(arrived: &mut Mut<'_, Self>, goal: Option<Vec3>) {
        arrived.set_if_neq(Self(goal));
    }
}

impl SimComponent for ArrivedAt {
    const NAME: &'static str = "navigation.arrived_at";
    type Host = UnitEntity;

    // Its goal lies within the world's bound, as every place does.
    fn check(&self, _: &World, _: Entity) -> bool {
        self.0.is_none_or(Vec3::within_bound)
    }
}

impl Replication for ArrivedAt {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
