use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_states::{NotSent, Replication};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::rally_target::RallyTarget;

/// A producer's rally point, where the units it trains go, or none: a part of every producer,
/// which a rally order writes in place.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Rally(Option<RallyTarget>);

impl Rally {
    pub const fn new(target: Option<RallyTarget>) -> Self {
        Self(target)
    }

    pub const fn get(self) -> Option<RallyTarget> {
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
