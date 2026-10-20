use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_declarations::PerType;
use campfire_sim::{SimComponent, TimedKept, TimedProcess, TimedRuns};
use campfire_states::{Replication, SentPredicted};
use campfire_units::{BuildTarget, UnitEntity};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::order_dues::{self, OrderDues, StandingOrder};

/// A unit that builds, as one whose slots hold a build: its build order, none while it has none.
/// Any other order ends it. Its hook pushes its unit into the index of the builders at each
/// insert, a spawn's and a restore's alike.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[component(on_insert = order_dues::pushed::<Builder>)]
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

    /// Gives it `order`, or none; with one, its unit is due at once in `dues`.
    pub(crate) fn set(&mut self, order: Option<BuildOrder>, dues: &mut OrderDues<'_, Self>) {
        self.0 = order;
        dues.push(self);
    }

    /// Gives the builder of `entity` in `world` `order`, or none, as an exclusive system does.
    pub(crate) fn set_in(world: &mut World, entity: Entity, order: Option<BuildOrder>) {
        order_dues::change(world, entity, |builder: &mut Self, dues| {
            builder.set(order, dues);
        });
    }
}

impl StandingOrder for Builder {
    fn has_order(&self) -> bool {
        self.0.is_some()
    }
}

/// A builder is due at once while it has an order, as a build order's walk, start and work run
/// in each tick (04-capabilities/00-overview.md, One due index for each timed process).
impl TimedProcess for Builder {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static Self;
    type Books = ();

    fn due((): &(), builder: &Self) -> Option<Tick> {
        builder.0.map(|_| Tick::ZERO)
    }
}

impl SimComponent for Builder {
    const NAME: &'static str = "production.builder";
    type Host = UnitEntity;

    // A slot past its unit's ends the order as its build starts, and a point or a site any
    // value names is checked as the order is.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Builder {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentPredicted;
}

impl PerType for Builder {
    const SCRIPTS_ONLY: bool = false;
}
