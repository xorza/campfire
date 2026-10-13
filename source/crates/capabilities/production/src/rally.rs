use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_core::UnitEntity;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, NotSent, Replication};
use serde::{Deserialize, Serialize};

use crate::rally_target::RallyTarget;

/// A producer's rally point, where the units it trains go; a producer with none has no component.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Rally(RallyTarget);

impl Rally {
    pub const fn new(target: RallyTarget) -> Rally {
        Rally(target)
    }

    pub const fn get(self) -> RallyTarget {
        self.0
    }
}

impl SimComponent for Rally {
    const NAME: &'static str = "production.rally";
    type Host = UnitEntity;

    // Its decode keeps a point a `Num`'s and names any stable id, which a train finds or not.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Rally {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
