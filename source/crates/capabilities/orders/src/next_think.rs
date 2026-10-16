use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, NotSent, Replication};
use campfire_units::UnitEntity;
use serde::{Deserialize, Serialize};

/// The tick a unit that thinks is due in again, a part of each unit of a type with AI from its
/// spawn: none before its first think, which comes in the first tick that leaves the remainder of
/// its stable id when divided by its type's period. A unit whose call found the think pool spent
/// keeps its tick, so it stays due, and goes first in the next tick, before the units due later.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NextThink(Option<Tick>);

impl NextThink {
    /// Before the unit's first think.
    pub const FIRST: Self = Self(None);

    pub const fn new(tick: Tick) -> Self {
        Self(Some(tick))
    }

    pub const fn get(self) -> Option<Tick> {
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
