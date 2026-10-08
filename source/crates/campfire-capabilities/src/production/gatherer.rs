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

    // A load of a resource the mode lacks would join no amount, and one with no last node has no
    // node to go back to; a loop's node is the last it chose. A step's tick of any value is
    // compared, not counted from.
    fn check(&self, world: &World, _: Entity) -> bool {
        let resources = world
            .get_resource::<PlayerResources>()
            .map_or(0, PlayerResources::resources);
        let load = self
            .load
            .is_none_or(|load| load.resource.index() < resources && self.last.is_some());
        let order = self.order.is_none_or(|order| self.last == Some(order.node));
        load && order
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::{Num, Vec3};
    use campfire_sim::IdAllocator;

    use super::*;
    use crate::values::declared_name::DeclaredName;

    #[test]
    fn a_restored_gatherer_carries_a_known_resource_with_a_node_to_go_back_to() {
        // A match of gold alone: wood, the second of two resources, is not its own.
        let mut world = World::new();
        world.insert_resource(PlayerResources::new(1, 1));
        let entity = world.spawn_empty().id();
        let names = ["gold", "wood"].map(|name| DeclaredName::new(name).unwrap());
        let [gold, wood] = ["gold", "wood"].map(|name| ResourceId::named(&names, name).unwrap());
        let mut ids = IdAllocator::default();
        let [first, second] = [0, 1].map(|z| NodeAt {
            node: ids.allocate(),
            at: Position::new(Vec3::new(Num::ZERO, Num::ZERO, Num::int(z))).unwrap(),
        });
        let load = |resource| Load {
            resource,
            amount: 5,
        };
        let order = |node| GatherOrder {
            slot: 0,
            node,
            step: GatherStep::ToNode,
        };
        let checks = |order, load, last| Gatherer { order, load, last }.check(&world, entity);
        assert!(checks(None, None, None));
        assert!(checks(None, Some(load(gold)), Some(first)));
        assert!(checks(Some(order(first)), None, Some(first)));
        assert!(!checks(None, Some(load(wood)), Some(first)));
        assert!(!checks(None, Some(load(gold)), None));
        assert!(!checks(Some(order(first)), None, Some(second)));
        assert!(!checks(Some(order(first)), None, None));
    }
}
