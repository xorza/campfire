use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_actions::{ActionBook, EffectListBook};
use campfire_common::Tick;
use campfire_declare::ByType;
use campfire_deliveries::Delivering;
use campfire_sim::{SimComponent, StableId};
use campfire_state_types::{DataKind, Replication, SentOnce};
use campfire_stats::Stats;
use campfire_units::DeliveryEntity;
use campfire_values::UnitType;
use serde::{Deserialize, Serialize};

use crate::area_spec::AreaSpec;

/// An area unit on the ground: the delivery it is, whose lists and hooks it runs, the unit
/// its action aimed at, if one, and the tick it ends. None of it changes; its `AreaTrigger`, until
/// it triggers, holds the tick it triggers.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
#[require(DeliveryEntity)]
pub struct Area {
    by: Delivering,
    aimed: Option<StableId>,
    ends_at: Tick,
}

impl Area {
    pub const fn new(by: Delivering, aimed: Option<StableId>, ends_at: Tick) -> Area {
        Area { by, aimed, ends_at }
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
}

impl SimComponent for Area {
    const NAME: &'static str = "areas.area";
    type Host = DeliveryEntity;

    // An area of a type with no area section, or of an action the book lacks or at a rank past
    // its ranks, or of a launch the match did not load, has no rules for its trigger or its
    // hooks; and one whose action does not give a param its inside modifiers read would fail as
    // a unit takes one.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let specs = world.resource::<ByType<AreaSpec>>();
        let spec = world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| specs.get(unit_type));
        let by = world
            .resource::<ActionBook>()
            .get(self.by.action)
            .is_some_and(|action| action.has_rank(self.by.rank));
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
        by && launched && spec.is_some_and(holds)
    }
}

impl Replication for Area {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}
