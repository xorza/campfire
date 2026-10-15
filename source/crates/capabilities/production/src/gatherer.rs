use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_declare::PerType;
use campfire_math::Vec3;
use campfire_players::PlayersWorld;
use campfire_sim::{Capability, Position, SimComponent, StableId};
use campfire_state_types::{DataKind, Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::{NameId, ResourceId};
use serde::{Deserialize, Serialize};

/// A unit that gathers, as one whose slots hold a gather: its gather loop, none while it has
/// none, the load it carries, which it keeps when its loop ends, and the node it gathered last,
/// which it goes back to after a return of its load.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gatherer {
    pub order: Option<GatherOrder>,
    pub load: Option<Load>,
    pub last: Option<NodeAt>,
}

/// A gather loop: the gather in `slot`, its node, and its step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatherOrder {
    pub slot: u8,
    pub node: NodeAt,
    pub step: GatherStep,
}

/// A node, and where it stood as the loop chose it, for the search of another near it once it is
/// gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeAt {
    pub node: StableId,
    pub pos: Vec3,
}

/// Where a worker is in its loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatherStep {
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
pub struct Load {
    pub resource: ResourceId,
    pub amount: u32,
}

impl Gatherer {
    pub const fn order(self) -> Option<GatherOrder> {
        self.order
    }

    pub const fn load(self) -> Option<Load> {
        self.load
    }

    pub const fn last(self) -> Option<NodeAt> {
        self.last
    }

    /// Sets its loop. A checked loop's node becomes the last it chose; an order not checked yet
    /// names a unit that may be no node, such as a drop-off, so the last stays.
    pub const fn set(&mut self, order: Option<GatherOrder>) {
        if let Some(order) = order
            && !matches!(order.step, GatherStep::Ordered)
        {
            self.last = Some(order.node);
        }
        self.order = order;
    }

    pub const fn carry(&mut self, load: Option<Load>) {
        self.load = load;
    }
}

impl SimComponent for Gatherer {
    const NAME: &'static str = "production.gatherer";
    type Host = UnitEntity;

    // A load of a resource the mode lacks would join no amount, and one with no last node has no
    // node to go back to; a checked loop's node is the last it chose. A step's tick of any value
    // is compared, not counted from; a node lies within the world's bound, as every place does.
    fn check(&self, world: &World, _: Entity) -> bool {
        let resources = world.declared_resources();
        let load = self
            .load
            .is_none_or(|load| load.resource.index() < resources && self.last.is_some());
        let order = self
            .order
            .is_none_or(|order| order.step == GatherStep::Ordered || self.last == Some(order.node));
        let node = |node: NodeAt| Position::from(node.pos).within_bound();
        let places = self.last.is_none_or(node) && self.order.is_none_or(|order| node(order.node));
        load && order && places
    }
}

impl Replication for Gatherer {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}

impl PerType for Gatherer {
    const OWNER: Option<Capability> = Some(Capability::Production);
    const SCRIPTS_ONLY: bool = false;
}
