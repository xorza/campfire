use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::actions::action_book::ActionBook;
use crate::actions::effect_lists::EffectLists;
use crate::areas::area_spec::AreaSpec;
use crate::deliveries::delivering::Delivering;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnce;
use crate::stats::modifier_book::ModifierBook;
use crate::units::by_type::ByType;
use crate::units::unit_type::UnitType;

/// An area unit on the ground: the delivery it is, whose lists and hooks it runs, the unit
/// its action aimed at, if one, and the tick it ends. None of it changes; its `AreaTrigger`, until
/// it triggers, holds the tick it triggers.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
pub struct Area {
    by: Delivering,
    aimed: Option<StableId>,
    ends_at: Tick,
}

impl Area {
    pub(crate) const fn new(by: Delivering, aimed: Option<StableId>, ends_at: Tick) -> Area {
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

    // An area of a type with no area section, or of an action the book lacks or at a rank past
    // its ranks, or of a launch the match did not load, has no rules for its trigger or its
    // hooks; and one whose action does not give a param its inside modifiers read would fail as
    // a unit takes one.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let spec = world
            .get::<UnitType>(entity)
            .zip(world.get_resource::<ByType<AreaSpec>>())
            .and_then(|(&unit_type, specs)| specs.get(unit_type));
        let book = world.get_resource::<ActionBook>();
        let by = book
            .and_then(|book| book.get(self.by.action))
            .is_some_and(|action| action.has_rank(self.by.rank));
        let Delivering {
            action,
            rank,
            launch,
            ..
        } = self.by;
        let launched = launch.is_none_or(|launch| {
            world
                .get_resource::<EffectLists>()
                .is_some_and(|lists| lists.has_launch(launch))
        });
        let applies = |modifier| ModifierBook::has_way_in(world, modifier, Some(action), rank);
        let holds = |spec: &AreaSpec| spec.inside.modifiers().all(applies);
        by && launched && spec.is_some_and(holds)
    }
}

impl Replication for Area {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}
