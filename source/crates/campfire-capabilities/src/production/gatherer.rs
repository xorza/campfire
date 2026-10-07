use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{Position, SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::players::player_resources::PlayerResources;
use crate::players::resource_id::ResourceId;

/// A unit that gathers, as one whose slots hold a gather: its gather loop, none while it has
/// none, the load it carries, which it keeps when its loop ends, and the node it gathered last,
/// which it goes back to after a return of its load.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Gatherer {
    order: Option<GatherOrder>,
    load: Option<Load>,
    last: Option<NodeAt>,
}

/// A gather loop: the gather in `slot`, its node, and its step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GatherOrder {
    pub(crate) slot: u8,
    pub(crate) node: NodeAt,
    pub(crate) step: GatherStep,
}

/// A node, and where it stood as the loop chose it, for the search of another near it once it is
/// gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NodeAt {
    pub(crate) node: StableId,
    pub(crate) at: Position,
}

/// Where a worker is in its loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum GatherStep {
    /// Its order applied this tick, aimed at its node, and waits for its check.
    Ordered,
    /// It walks to its node.
    ToNode,
    /// It waits at its node, which another holds, from the tick `since`.
    Waiting { since: Tick },
    /// It holds its node and gathers, from the tick `since`.
    Gathering { since: Tick },
    /// It takes its load to the drop-off it chose, none until it chooses one.
    ToDropOff { drop_off: Option<StableId> },
}

/// What a worker carries: an amount of one player resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Load {
    pub(crate) resource: ResourceId,
    pub(crate) amount: u32,
}

impl Gatherer {
    pub(crate) const fn order(self) -> Option<GatherOrder> {
        self.order
    }

    pub(crate) const fn load(self) -> Option<Load> {
        self.load
    }

    pub(crate) const fn last(self) -> Option<NodeAt> {
        self.last
    }

    pub(crate) const fn set(&mut self, order: Option<GatherOrder>) {
        if let Some(order) = order {
            self.last = Some(order.node);
        }
        self.order = order;
    }

    pub(crate) const fn carry(&mut self, load: Option<Load>) {
        self.load = load;
    }
}

impl SimComponent for Gatherer {
    const NAME: &'static str = "production.gatherer";

    // A load of a resource the mode lacks would join no amount; a step's tick of any value is
    // compared, not counted from.
    fn check(&self, world: &World, _: Entity) -> bool {
        let resources = world
            .get_resource::<PlayerResources>()
            .map_or(0, PlayerResources::resources);
        self.load
            .is_none_or(|load| load.resource.index() < resources)
    }
}
