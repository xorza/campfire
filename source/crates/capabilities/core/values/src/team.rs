use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{OnAny, SimComponent};
use campfire_state_types::{DataKind, Replication, SentOnInsert};
use serde::{Deserialize, Serialize};

use crate::bits256::Bits256;
use crate::name_kind::{Article, NameKind, NameScope, Named};
use crate::names::NameId;

/// A unit's team: its index in the mode's list of teams. How two teams regard each other is
/// their relation, which the match's relations hold.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[component(immutable)]
#[serde(transparent)]
pub struct Team(u8);

impl Team {
    pub const fn new(index: u8) -> Self {
        Self(index)
    }

    /// The team as the wire and the session's receipt write it.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl NameId for Team {
    type Scope = ();
    /// The most teams a match holds: one bit each in a `TeamSet`, and every index a `u8` holds.
    const LIMIT: usize = Bits256::BITS;

    fn at(index: usize) -> Self {
        Self(u8::try_from(index).expect("teams fit u8"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl SimComponent for Team {
    const NAME: &'static str = "units.team";
    type Host = OnAny;

    // `units` declares its host, a unit or a player, and checks it against the mode's teams.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

// Sent on each insert, as a change of owner moves its unit to the new owner's team
// (14-replication-audiences.md, A unit's owner is a player of its team).
impl Replication for Team {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnInsert;
}

impl Named for Team {
    const NAMES: NameKind = NameKind::new("team", Article::A, NameScope::Match);
}
