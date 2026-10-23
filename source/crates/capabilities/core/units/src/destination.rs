use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Vec3;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;

/// Where a unit walks to; none once it arrives. The goal of a walk, which a capability sets and
/// `navigation` pursues and drops on arrival: navigation's state, kept here, as `ForcedMove` is, so
/// a capability walks a unit with no dependency on navigation (02-engine-core.md, Contracts through
/// `units`).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Destination(Option<Vec3>);

impl Destination {
    pub(crate) const fn of(target: Option<Vec3>) -> Self {
        Self(target)
    }

    pub const fn get(self) -> Option<Vec3> {
        self.0
    }

    /// Sets where a unit walks, as navigation and an order, which hold the unit's route, do: the
    /// route keeps the goal it arrived short of, so an order that sends the unit there again
    /// plans nothing, and a walk that the static bodies open goes on. A capability walks a unit
    /// by `Walk`, which keeps the rule of a short arrival.
    pub fn route_to(destination: &mut Mut<'_, Self>, target: Option<Vec3>) {
        destination.set_if_neq(Self(target));
    }
}

impl SimComponent for Destination {
    const NAME: &'static str = "units.destination";
    type Host = UnitEntity;

    // Its point lies within the world's bound, as every place does; the cells a unit walks over
    // to it are its route's, whose check holds them to a kind of walker the mode has.
    fn check(&self, _: &World, _: Entity) -> bool {
        self.0.is_none_or(Vec3::within_bound)
    }
}

impl Replication for Destination {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Vec3;

    use crate::destination::Destination;

    impl Destination {
        /// A destination of `target`, none for no target, as a test sets a unit's walk.
        pub const fn to(target: Option<Vec3>) -> Self {
            Self(target)
        }
    }
}
