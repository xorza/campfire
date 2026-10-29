use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Vec3;
use campfire_sim::{SimComponent, StableId};
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
pub struct Destination(Option<WalkGoal>);

/// Where a walk goes: to a point, or into the unit `unit`, at `at`, the place its capability took
/// for the unit last, which the walker passes through as it walks into it (entries-and-blocking.md,
/// A walk may go into a unit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WalkGoal {
    Point(Vec3),
    Into { unit: StableId, at: Vec3 },
}

impl WalkGoal {
    /// The point it goes to: the unit's place for a walk into one.
    pub const fn at(self) -> Vec3 {
        match self {
            Self::Point(at) | Self::Into { at, .. } => at,
        }
    }

    /// The unit it goes into, if any.
    pub const fn unit(self) -> Option<StableId> {
        match self {
            Self::Point(_) => None,
            Self::Into { unit, .. } => Some(unit),
        }
    }
}

impl Destination {
    pub(crate) const fn of(goal: Option<WalkGoal>) -> Self {
        Self(goal)
    }

    /// The point it goes to, a unit's place for a walk into one.
    pub const fn get(self) -> Option<Vec3> {
        match self.0 {
            Some(goal) => Some(goal.at()),
            None => None,
        }
    }

    pub const fn goal(self) -> Option<WalkGoal> {
        self.0
    }

    /// The unit the walk goes into, if any.
    pub const fn unit(self) -> Option<StableId> {
        match self.0 {
            Some(goal) => goal.unit(),
            None => None,
        }
    }

    /// Sets where a unit walks, as navigation and an order, which hold the unit's route, do: the
    /// route keeps the goal it arrived short of, so an order that sends the unit there again
    /// plans nothing, and a walk that the static bodies open goes on. A capability walks a unit
    /// by `Walk`, which keeps the rule of a short arrival.
    pub fn route_to(destination: &mut Mut<'_, Self>, target: Option<Vec3>) {
        destination.set_if_neq(Self(target.map(WalkGoal::Point)));
    }
}

impl SimComponent for Destination {
    const NAME: &'static str = "units.destination";
    type Host = UnitEntity;

    // Its point lies within the world's bound, as every place does; the cells a unit walks over
    // to it are its route's, whose check holds them to a kind of walker the mode has.
    fn check(&self, _: &World, _: Entity) -> bool {
        self.get().is_none_or(Vec3::within_bound)
    }
}

impl Replication for Destination {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Vec3;
    use campfire_sim::StableId;

    use crate::destination::{Destination, WalkGoal};

    impl Destination {
        /// A destination of `target`, none for no target, as a test sets a unit's walk.
        pub const fn to(target: Option<Vec3>) -> Self {
            match target {
                Some(at) => Self(Some(WalkGoal::Point(at))),
                None => Self(None),
            }
        }

        /// A destination into `unit`, which stands at `at`, as a test sets a unit's walk.
        pub const fn to_unit(unit: StableId, at: Vec3) -> Self {
            Self(Some(WalkGoal::Into { unit, at }))
        }
    }
}
