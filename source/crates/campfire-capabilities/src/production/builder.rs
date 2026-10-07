use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::production::build_target::BuildTarget;

/// A unit that builds, as one whose slots hold a build: its build order, none while it has none.
/// Any other order ends it.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Builder(Option<BuildOrder>);

/// A build order: the build in `slot`, and where it builds, a point until its site stands, then
/// that site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildOrder {
    pub slot: u8,
    pub target: BuildTarget,
}

impl Builder {
    pub const fn order(self) -> Option<BuildOrder> {
        self.0
    }

    pub(crate) const fn set(&mut self, order: Option<BuildOrder>) {
        self.0 = order;
    }
}

impl SimComponent for Builder {
    const NAME: &'static str = "production.builder";

    // A slot past its unit's ends the order as its build starts, and a point or a site any
    // value names is checked as the order is.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}
