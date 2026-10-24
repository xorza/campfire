use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Changed;
use bevy_ecs::system::Query;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_declarations::PerId;
use campfire_sim::{DueIndex, SimComponent, TimedKept, TimedProcess, TimedRuns, UnitDues};
use campfire_states::{Replication, SentPredicted};
use campfire_units::{AtWork, BuildTarget, UnitEntity};
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::order_dues::{self, StandingOrder};

/// A unit that builds, as one whose slots hold a build: its build order, none while it has none.
/// Any other order ends it. Its hook pushes its unit into the index of the builders at each
/// insert, a spawn's and a restore's alike.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[component(on_insert = DueIndex::<Self>::pushed)]
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
    pub(crate) fn set(&mut self, order: Option<BuildOrder>, dues: &mut UnitDues<'_, Self>) {
        self.0 = order;
        dues.push(Self::due(&(), self));
    }

    /// Shows in each builder's `AtWork` whether it holds a build, once its order changed, so what
    /// moves an idle unit aside leaves it (04-capabilities/navigation.md, An idle friend steps
    /// aside).
    pub(crate) fn show_work(mut builders: Query<'_, '_, (&Self, &mut AtWork), Changed<Self>>) {
        for (builder, mut at_work) in &mut builders {
            at_work.set_if_neq(AtWork::new(builder.0.is_some()));
        }
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

impl PerId<UnitType> for Builder {
    const SCRIPTS_ONLY: bool = false;
}
