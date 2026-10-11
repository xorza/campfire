use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::state_types::data_kind::DataKind;
use crate::state_types::kinded::Kinded;
use crate::units::bits256::Bits256;
use crate::units::view::View;

/// A unit's team: its index in the mode's list of teams. How two teams regard each other is
/// their relation, which the match's relations hold.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Team(u8);

impl Team {
    /// The most teams a match holds: one bit each in a `TeamSet`, and every index a `u8` holds.
    pub(crate) const LIMIT: usize = Bits256::BITS;

    pub const fn new(index: u8) -> Team {
        Team(index)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// The team as the wire and the session's receipt write it.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl SimComponent for Team {
    const NAME: &'static str = "units.team";

    // The vision groups and the mode's tables hold a place for each of the mode's teams only.
    fn check(&self, world: &World, _: Entity) -> bool {
        world
            .get_non_send::<View>()
            .is_none_or(|view| view.has_team(*self))
    }
}

impl Kinded for Team {
    const KIND: DataKind = DataKind::Unit;
}
