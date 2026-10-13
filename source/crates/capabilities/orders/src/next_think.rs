use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_core::UnitEntity;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, NotSent, Replication};
use serde::{Deserialize, Serialize};

/// The tick a unit that thinks is due in again. A unit whose call found the think pool spent
/// keeps it, so it stays due, and goes first in the next tick, before the units due later.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NextThink(Tick);

impl NextThink {
    pub const fn new(tick: Tick) -> NextThink {
        NextThink(tick)
    }

    pub const fn get(self) -> Tick {
        self.0
    }
}

impl SimComponent for NextThink {
    const NAME: &'static str = "orders.next_think";
    type Host = UnitEntity;

    // Its decode keeps its tick within the limit.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for NextThink {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
