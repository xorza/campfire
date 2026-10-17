use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Vec3;
use campfire_sim::{Position, SimComponent};
use campfire_state_types::{DataKind, Replication, SentPredicted};
use serde::{Deserialize, Serialize};

use crate::arrived_short::ArrivedShort;
use crate::unit_entity::UnitEntity;

/// Where a unit walks to; none once it arrives. The goal of a walk, which a capability sets and
/// `navigation` pursues and drops on arrival: navigation's state, kept here, as `ForcedMove` is, so
/// a capability walks a unit with no dependency on navigation (02-engine-core.md, Contracts through
/// `units`).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Destination(Option<Vec3>);

impl Destination {
    /// A destination of `target`, none for no target.
    pub const fn to(target: Option<Vec3>) -> Self {
        Self(target)
    }

    pub const fn get(self) -> Option<Vec3> {
        self.0
    }

    /// Sets where a unit walks, leaving a destination that does not change untouched: a write
    /// marks it changed, and an avatar's destination replicates. A unit that `arrived` short of
    /// `target` stays where it stands, with no destination, until the static bodies change.
    pub fn walk_to(
        destination: &mut Mut<'_, Self>,
        arrived: Option<&ArrivedShort>,
        target: Option<Vec3>,
    ) {
        let short =
            target.is_some_and(|target| arrived.is_some_and(|arrived| arrived.of_goal(target)));
        if !short {
            destination.set_if_neq(Self::to(target));
        }
    }

    /// Sets where the unit of `entity` walks, when it walks, to `target`, leaving a destination
    /// that does not change untouched, as a capability's own loop walks its unit.
    pub fn go_to(world: &mut World, entity: Entity, target: Option<Vec3>) {
        if let Some(mut destination) = world.get_mut::<Self>(entity) {
            destination.set_if_neq(Self::to(target));
        }
    }

    /// Whether a unit with `destination` gives up a walk to `target`: it cannot walk, or it
    /// stands, as it `arrived` short of `target`.
    pub fn gives_up(
        destination: Option<&Self>,
        arrived: Option<&ArrivedShort>,
        target: Vec3,
    ) -> bool {
        destination.is_none_or(|destination| {
            destination.get().is_none() && arrived.is_some_and(|arrived| arrived.of_goal(target))
        })
    }
}

impl SimComponent for Destination {
    const NAME: &'static str = "units.destination";
    type Host = UnitEntity;

    // Its point lies within the world's bound, as every place does; the cells a unit walks over
    // to it are its route's, whose check holds them to a kind of walker the mode has.
    fn check(&self, _: &World, _: Entity) -> bool {
        self.0
            .is_none_or(|point| Position::from(point).within_bound())
    }
}

impl Replication for Destination {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
