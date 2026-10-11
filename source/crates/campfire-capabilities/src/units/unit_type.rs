use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::scripts::state_decl::synced_state_decl::SyncTo;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnce;
use crate::units::unit_entity::UnitEntity;
use crate::units::unit_state::{SyncAll, SyncNone, SyncOwner, SyncTeam, UnitState};
use crate::units::unit_state_book::UnitStateBook;
use crate::units::view::View;

/// A unit's type, by its place in the match's unit types.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[component(immutable)]
#[require(UnitEntity)]
#[serde(transparent)]
pub struct UnitType(u16);

impl UnitType {
    pub(crate) const fn new(index: u16) -> UnitType {
        UnitType(index)
    }

    /// Its place in the match's unit types, from 0.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl SimComponent for UnitType {
    const NAME: &'static str = "units.unit_type";
    type Host = UnitEntity;

    // A type the match did not load names nothing its books hold, and a unit without a class of
    // the state its type declares would read a field it lacks.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let known = world
            .get_non_send::<View>()
            .is_none_or(|view| view.has_type(*self));
        let unit = world.entity(entity);
        let states = || {
            let book = world.resource::<UnitStateBook>();
            book.fields(*self).iter().all(|field| match field.sync {
                SyncTo::None => unit.contains::<UnitState<SyncNone>>(),
                SyncTo::Owner => unit.contains::<UnitState<SyncOwner>>(),
                SyncTo::Team => unit.contains::<UnitState<SyncTeam>>(),
                SyncTo::All => unit.contains::<UnitState<SyncAll>>(),
            })
        };
        known && states()
    }
}

impl Replication for UnitType {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}
