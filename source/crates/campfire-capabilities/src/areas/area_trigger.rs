use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::areas::area::Area;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::NotSent;

/// The tick an area triggers, which it holds until it does. No client reads it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct AreaTrigger(Tick);

impl AreaTrigger {
    pub(crate) const fn at(tick: Tick) -> AreaTrigger {
        AreaTrigger(tick)
    }

    pub(crate) const fn get(self) -> Tick {
        self.0
    }
}

impl SimComponent for AreaTrigger {
    const NAME: &'static str = "areas.area_trigger";

    // An area's, which it triggers before it ends.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world
            .get::<Area>(entity)
            .is_some_and(|area| self.0 <= area.ends_at())
    }
}

impl Replication for AreaTrigger {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
