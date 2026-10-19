use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_declare::PerType;
use campfire_math::Vec3;
use campfire_players::PlayersWorld;
use campfire_sim::{SimComponent, StableId, TimedKept, TimedProcess, TimedRuns};
use campfire_state_types::{DataKind, Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::{NameId, ResourceId};
use serde::{Deserialize, Serialize};

use crate::order_dues::{self, OrderDues, StandingOrder};

/// A unit that gathers, as one whose slots hold a gather: its gather loop, none while it has
/// none, the load it carries, which it keeps when its loop ends, and the node it gathered last,
/// which it goes back to after a return of its load. Its hook pushes its unit into the index of
/// the gatherers at each insert, a spawn's and a restore's alike.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[component(on_insert = order_dues::pushed::<Gatherer>)]
pub struct Gatherer {
    order: Option<GatherOrder>,
    load: Option<Load>,
    last: Option<NodeAt>,
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
    pub position: Vec3,
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

    /// Sets its loop, its unit due at once in `dues` with one. A checked loop's node becomes the
    /// last it chose; an order not checked yet names a unit that may be no node, such as a
    /// drop-off, so the last stays.
    pub(crate) fn set(&mut self, order: Option<GatherOrder>, dues: &mut OrderDues<'_, Self>) {
        if let Some(order) = order
            && !matches!(order.step, GatherStep::Ordered)
        {
            self.last = Some(order.node);
        }
        self.order = order;
        dues.push(self);
    }

    /// Sets the loop of the gatherer of `entity` in `world`, as an exclusive system does.
    pub(crate) fn set_in(world: &mut World, entity: Entity, order: Option<GatherOrder>) {
        order_dues::change(world, entity, |gatherer: &mut Self, dues| {
            gatherer.set(order, dues);
        });
    }

    pub const fn carry(&mut self, load: Option<Load>) {
        self.load = load;
    }
}

impl StandingOrder for Gatherer {
    fn has_order(&self) -> bool {
        self.order.is_some()
    }
}

/// A gatherer is due at once while it has a loop, as each step of a loop runs in each tick
/// (04-capabilities/00-overview.md, One due index for each timed process).
impl TimedProcess for Gatherer {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static Self;
    type Books = ();

    fn due((): &(), gatherer: &Self) -> Option<Tick> {
        gatherer.order.map(|_| Tick::ZERO)
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
        let node = |node: NodeAt| node.position.within_bound();
        let places = self.last.is_none_or(node) && self.order.is_none_or(|order| node(order.node));
        load && order && places
    }
}

impl Replication for Gatherer {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}

impl PerType for Gatherer {
    const SCRIPTS_ONLY: bool = false;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::entity::Entity;
    use campfire_sim::{DueIndex, StableId};

    use crate::gatherer::{GatherOrder, Gatherer, Load, NodeAt};
    use crate::order_dues::OrderDues;

    impl Gatherer {
        /// Sets its loop as `set` does, with no index: for a gatherer a test or a bench makes,
        /// whose insert then pushes its unit, and for a loop a test ends, which pushes nothing.
        pub fn set_unindexed(&mut self, order: Option<GatherOrder>) {
            let mut index = DueIndex::default();
            self.set(
                order,
                &mut OrderDues::new(&mut index, StableId::MATCH, Entity::PLACEHOLDER),
            );
        }

        /// A gatherer of `order`, `load` and `last` as given, whether they keep the rules `set`
        /// keeps or not, as a restore may read one.
        pub const fn of_fields(
            order: Option<GatherOrder>,
            load: Option<Load>,
            last: Option<NodeAt>,
        ) -> Self {
            Self { order, load, last }
        }
    }
}
