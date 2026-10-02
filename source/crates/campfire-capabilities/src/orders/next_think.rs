use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// The tick a unit that thinks is due in again. A unit whose call found the think pool spent
/// keeps it, so it stays due, and goes first in the next tick, before the units due later.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct NextThink(Tick);

impl NextThink {
    pub(crate) const fn new(tick: Tick) -> NextThink {
        NextThink(tick)
    }

    pub(crate) const fn get(self) -> Tick {
        self.0
    }
}

impl SimComponent for NextThink {
    const NAME: &'static str = "orders.next_think";

    // A tick, which every match may reach.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}
