use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{Either, SimComponent};
use serde::{Deserialize, Serialize};

use crate::scripts::name_kind::NameKind;
use crate::scripts::script_arg::name_script_arg;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnInsert;
use crate::units::bits256::Bits256;
use crate::units::player::Player;
use crate::units::unit_entity::UnitEntity;
use crate::units::view::View;
use crate::values::names::NameId;

/// A unit's team: its index in the mode's list of teams. How two teams regard each other is
/// their relation, which the match's relations hold.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[component(immutable)]
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

impl NameId for Team {
    type Scope = ();
    const LIMIT: usize = Team::LIMIT;

    fn at(index: usize) -> Team {
        Team(u8::try_from(index).expect("teams fit u8"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl SimComponent for Team {
    const NAME: &'static str = "units.team";
    type Host = Either<UnitEntity, Player>;

    // The vision groups and the mode's tables hold a place for each of the mode's teams only.
    fn check(&self, world: &World, _: Entity) -> bool {
        world.non_send::<View>().names().has_team(*self)
    }
}

// Sent on each insert, as a change of owner moves its unit to the new owner's team (design 14,
// D18).
impl Replication for Team {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnInsert;
}

name_script_arg!(Team, NameKind::Team, |ctx, name| {
    ctx.view().names().team_names().named(name)
});
