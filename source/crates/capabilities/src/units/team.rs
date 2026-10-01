use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::values::bits256::Bits256;

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

    pub const fn index(self) -> u8 {
        self.0
    }
}

impl SimComponent for Team {
    const NAME: &'static str = "units.team";
}
