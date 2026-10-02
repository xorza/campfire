use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{SimComponent, StableId};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::actions::action_book::ActionBook;
use crate::areas::area_spec::AreaSpec;
use crate::deliveries::delivering::Delivering;
use crate::stats::modifier_book::ModifierBook;
use crate::units::by_type::ByType;
use crate::units::unit_type::UnitType;

/// An area unit on the ground: the delivery it is, whose `on_hit` and `on_end` it runs, the unit
/// its action aimed at, if one, the tick it triggers, `None` once it did, and the tick it ends,
/// never before its trigger.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Area {
    by: Delivering,
    aimed: Option<StableId>,
    triggers_at: Option<Tick>,
    ends_at: Tick,
}

impl Area {
    /// `None` when it would trigger after it ends.
    pub(crate) fn new(
        by: Delivering,
        aimed: Option<StableId>,
        triggers_at: Option<Tick>,
        ends_at: Tick,
    ) -> Option<Area> {
        triggers_at.is_none_or(|at| at <= ends_at).then_some(Area {
            by,
            aimed,
            triggers_at,
            ends_at,
        })
    }

    pub(crate) const fn by(&self) -> Delivering {
        self.by
    }

    pub(crate) const fn aimed(&self) -> Option<StableId> {
        self.aimed
    }

    pub(crate) const fn triggers_at(&self) -> Option<Tick> {
        self.triggers_at
    }

    pub(crate) const fn ends_at(&self) -> Tick {
        self.ends_at
    }

    /// Counts its trigger as done.
    pub(crate) const fn trigger(&mut self) {
        self.triggers_at = None;
    }
}

impl SimComponent for Area {
    const NAME: &'static str = "areas.area";

    // An area of a type with no area section, or of an action the book lacks or at a rank past
    // its ranks, has no rules for its trigger or its hooks; and one whose action does not give a
    // param its inside modifiers read would fail as a unit takes one.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let spec = world
            .get::<UnitType>(entity)
            .zip(world.get_resource::<ByType<AreaSpec>>())
            .and_then(|(&unit_type, specs)| specs.get(unit_type));
        let book = world.get_resource::<ActionBook>();
        let by = book
            .and_then(|book| book.get(self.by.action))
            .is_some_and(|action| action.has_rank(self.by.rank));
        let Delivering { action, rank, .. } = self.by;
        let applies = |modifier| ModifierBook::has_way_in(world, modifier, Some(action), rank);
        let holds = |spec: &AreaSpec| spec.inside.modifiers().all(applies);
        let times =
            self.triggers_at.is_none_or(|at| at <= Tick::LIMIT) && self.ends_at <= Tick::LIMIT;
        by && spec.is_some_and(holds) && times
    }
}

/// A snapshot is untrusted, so an area that would trigger after it ends fails to decode.
impl<'de> Deserialize<'de> for Area {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Area, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            by: Delivering,
            aimed: Option<StableId>,
            triggers_at: Option<Tick>,
            ends_at: Tick,
        }
        let fields = Fields::deserialize(deserializer)?;
        Area::new(fields.by, fields.aimed, fields.triggers_at, fields.ends_at)
            .ok_or_else(|| D::Error::custom("an area that triggers after it ends"))
    }
}
