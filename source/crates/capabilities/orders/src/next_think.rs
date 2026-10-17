use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_declare::ByType;
use campfire_sim::{SimComponent, SimTick};
use campfire_state_types::{DataKind, NotSent, Replication};
use campfire_units::UnitEntity;
use campfire_values::UnitType;
use serde::{Deserialize, Serialize};

use crate::ai::Ai;

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

    // A think sets the next a period on, so a unit of a type with AI is due at most a period
    // after the tick the match runs next; a later tick would keep its AI from thinking.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let Some(next) = self.0 else {
            return true;
        };
        let now = world.resource::<SimTick>().start();
        let ai = world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| world.resource::<ByType<Ai>>().get(unit_type));
        ai.is_some_and(|ai| next <= now.after(ai.period))
    }
}

impl Replication for NextThink {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
