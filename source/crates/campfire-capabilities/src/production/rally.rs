use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::production::rally_target::RallyTarget;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::NotSent;

/// A producer's rally point, where the units it trains go; a producer with none has no component.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Rally(RallyTarget);

impl Rally {
    pub(crate) const fn new(target: RallyTarget) -> Rally {
        Rally(target)
    }

    pub const fn get(self) -> RallyTarget {
        self.0
    }
}

impl SimComponent for Rally {
    const NAME: &'static str = "production.rally";

    // Its decode keeps a point a `Num`'s and names any stable id, which a train finds or not.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Rally {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
