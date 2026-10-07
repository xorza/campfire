use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::units::script_view::View;

/// A unit's type, by its place in the match's unit types.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnitType(u16);

impl UnitType {
    pub(crate) const fn new(index: u16) -> UnitType {
        UnitType(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

impl SimComponent for UnitType {
    const NAME: &'static str = "units.unit_type";

    // A type the match did not load names nothing its books hold.
    fn check(&self, world: &World, _: Entity) -> bool {
        world
            .get_non_send::<View>()
            .is_none_or(|view| view.has_type(*self))
    }
}
