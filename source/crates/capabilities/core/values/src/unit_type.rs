use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{OnAny, SimComponent};
use campfire_state_types::{DataKind, Replication, SentOnce};
use serde::{Deserialize, Serialize};

use crate::names::NameId;
use crate::type_scope::TypeScope;
use crate::unit_tags::UnitTags;

/// A unit's type, by its place in the match's unit types. Its tags, which it requires, come with
/// it on every path, a spawn, a restore and a client's replication, as the hook `units` gives it
/// on insert writes them.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[component(immutable)]
#[require(UnitTags)]
#[serde(transparent)]
pub struct UnitType(u16);

impl UnitType {
    pub const fn new(index: u16) -> UnitType {
        UnitType(index)
    }

    /// Its place in the match's unit types, from 0.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl NameId for UnitType {
    type Scope = TypeScope;
    const LIMIT: usize = 1 << u16::BITS;

    fn at(index: usize) -> UnitType {
        UnitType(u16::try_from(index).expect("unit types fit u16"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl SimComponent for UnitType {
    const NAME: &'static str = "units.unit_type";
    type Host = OnAny;

    // `units` declares its host, a unit, and checks it against the match's types and states.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for UnitType {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}
