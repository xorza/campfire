use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::{DeferredWorld, World};
use campfire_actions::{ActionBook, EffectListBook};
use campfire_common::Tick;
use campfire_declarations::ById;
use campfire_deliveries::Delivering;
use campfire_sim::{DueIndex, SimComponent, StableId, TimedKept, TimedProcess, TimedRuns};
use campfire_states::{NotSent, Replication, SentOnce};
use campfire_stats::Stats;
use campfire_units::DeliveryEntity;
use campfire_values::{ActionStart, UnitType};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::area_spec::AreaSpec;

/// An area unit on the ground: the delivery it is, whose lists and hooks it runs, the unit
/// its action aimed at, if one, and the tick it ends. None of it changes; its `AreaTrigger`, until
/// it triggers, holds the tick it triggers. Its hook pushes it into the index of the areas at
/// each insert, a spawn's and a restore's alike.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable, on_insert = Area::pushed)]
#[require(DeliveryEntity)]
pub struct Area {
    by: Delivering,
    aimed: Option<StableId>,
    ends_at: Tick,
}

impl Area {
    pub const fn new(by: Delivering, aimed: Option<StableId>, ends_at: Tick) -> Self {
        Self { by, aimed, ends_at }
    }

    pub(crate) const fn by(&self) -> Delivering {
        self.by
    }

    pub(crate) const fn aimed(&self) -> Option<StableId> {
        self.aimed
    }

    pub(crate) const fn ends_at(&self) -> Tick {
        self.ends_at
    }

    /// Pushes the area into the index of the areas, at its trigger or its end, in a match that
    /// runs them.
    fn pushed(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = context.entity;
        let unit = world.entity(entity);
        let (Some(area), Some(&id)) = (unit.get::<Self>(), unit.get::<StableId>()) else {
            return;
        };
        let at = Self::due(&(), (area, unit.get::<AreaTrigger>()));
        if let (Some(at), Some(mut index)) = (at, world.get_resource_mut::<DueIndex<Self>>()) {
            index.push(at, id, entity);
        }
    }
}

/// The tick an area triggers, which it holds until it does. No client reads it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AreaTrigger(Tick);

impl AreaTrigger {
    pub const fn at(tick: Tick) -> Self {
        Self(tick)
    }

    pub const fn get(self) -> Tick {
        self.0
    }
}

/// An area is due at its trigger while it holds one, then at its end
/// (04-capabilities/00-overview.md, One due index for each timed process).
impl TimedProcess for Area {
    const RUNS: TimedRuns = TimedRuns::Server;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = (&'static Self, Option<&'static AreaTrigger>);
    type Books = ();

    fn due((): &(), (area, trigger): (&Self, Option<&AreaTrigger>)) -> Option<Tick> {
        Some(trigger.map_or(area.ends_at, |trigger| trigger.get()))
    }
}

impl SimComponent for Area {
    const NAME: &'static str = "areas.area";
    type Host = DeliveryEntity;

    // An area of a type with no area section, or of an action the book lacks or at a rank past
    // its ranks, or of a launch the match did not load, has no rules for its trigger or its
    // hooks; and one whose action does not give a param its inside modifiers read would fail as
    // a unit takes one; its action's start lies within the world's bound, as every place does.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let specs = world.resource::<ById<UnitType, AreaSpec>>();
        let spec = world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| specs.get(unit_type));
        let by = world
            .resource::<ActionBook>()
            .is_ranked(self.by.action, self.by.rank);
        let Delivering {
            action,
            rank,
            launch,
            ..
        } = self.by;
        let launched = launch.is_none_or(|launch| {
            world
                .get_resource::<EffectListBook>()
                .is_some_and(|lists| lists.has_launch(launch))
        });
        let applies = |modifier| Stats::modifier_has_way(world, modifier, Some(action), rank);
        let holds = |spec: &AreaSpec| spec.inside.modifiers().all(applies);
        let start = self.by.start.is_none_or(ActionStart::check);
        by && launched && spec.is_some_and(holds) && start
    }
}

impl Replication for Area {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}

impl SimComponent for AreaTrigger {
    const NAME: &'static str = "areas.area_trigger";
    type Host = DeliveryEntity;

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
