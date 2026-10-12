use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::World;
use campfire_sim::{Capability, Position, SimSet, StableId};

use crate::actions::action_target::ActionTarget;
use crate::actions::delivery::Delivery;
use crate::actions::effect_queues::EffectQueues;
use crate::areas::area::Area;
use crate::areas::area_holds::AreaHolds;
use crate::areas::area_launches::{AreaLaunch, AreaLaunches};
use crate::areas::area_life::AreaLife;
use crate::areas::area_spec::AreaSpec;
use crate::areas::area_trigger::AreaTrigger;
use crate::areas::areas_api::AreasApi;
use crate::areas::areas_effect::AreasEffect;
use crate::combat::CombatSet;
use crate::declare::declarations::Declarations;
use crate::declare::declares::Declares;
use crate::declare::install_cx::InstallCx;
use crate::deliveries::DeliverySet;
use crate::deliveries::deliverers::Deliverers;
use crate::deliveries::delivering::Delivering;
use crate::stats::StatsSet;
use crate::units::by_type::ByType;
use crate::units::unit_type::UnitType;

pub(crate) mod area;
pub(crate) mod area_data;
pub(crate) mod area_holds;
pub(crate) mod area_launches;
pub(crate) mod area_life;
pub(crate) mod area_reach;
pub(crate) mod area_spec;
pub(crate) mod area_trigger;
pub(crate) mod areas_api;
pub(crate) mod areas_effect;
pub(crate) mod areas_world;

/// The `areas` capability: area units, which actions deliver. It builds on combat, which a match
/// installs too.
#[derive(Debug)]
pub struct Areas;

impl Declares for Areas {
    const MODULE: Option<Capability> = Some(Capability::Areas);

    fn declare<D: Declarations>(d: &mut D) {
        d.component::<Area>();
        d.component::<AreaTrigger>();
        d.book::<ByType<AreaSpec>>();
        d.api(AreasApi::register);
    }
}

impl Areas {
    /// Adds areas to a match. In Hit, after projectiles fly, each area due triggers, and its
    /// reaches and its end run their action's `on_hit` and `on_end`, or its launch's; after the
    /// tick's projectiles launch, its areas land. In Resolve, the areas the damage pass launched
    /// land, and then, before stats hold their modifiers, each area lists those it holds on the
    /// units inside. A launch queues as an effect of `areas`.
    pub(crate) fn install(cx: &mut InstallCx<'_>) {
        let (world, schedule) = (&mut *cx.world, &mut *cx.schedule);
        world
            .resource_mut::<Deliverers>()
            .register_area(Areas::deliver);
        world.insert_resource(AreaLaunches::default());
        world
            .resource_mut::<EffectQueues>()
            .register(Capability::Areas, AreasEffect::queue_listed);
        schedule.add_systems((
            AreaLife::trigger.in_set(DeliverySet::Trigger),
            AreaLife::land
                .in_set(SimSet::Hit)
                .after(CombatSet::Launch)
                .before(CombatSet::Interval),
            AreaLife::land
                .in_set(SimSet::Resolve)
                .after(CombatSet::Damage)
                .before(CombatSet::Die),
            AreaHolds::hold_inside
                .in_set(SimSet::Resolve)
                .after(CombatSet::Die)
                .before(StatsSet::Hold),
        ));
    }

    /// Lands the area of `unit_type` of `by`, which aimed at `target` from `from`: on the point it
    /// aimed at, where the unit it aimed at stands, or at `from` for an action that aims at
    /// nothing; nothing for a unit that is gone.
    fn deliver(
        world: &mut World,
        by: Delivering,
        from: Position,
        delivery: Delivery,
        target: ActionTarget,
    ) {
        let unit_type = delivery.unit_type;
        let at = match target.point(world) {
            Some(at) => at,
            None if target == ActionTarget::None => from,
            None => return,
        };
        Areas::push(world, by, unit_type, at, target.unit(), None);
    }

    /// Queues an area of `unit_type` of `by` to land at `pos`, aimed at `aimed`; with `id`, the id
    /// the script that placed it took for it.
    fn push(
        world: &mut World,
        by: Delivering,
        unit_type: UnitType,
        pos: Position,
        aimed: Option<StableId>,
        id: Option<StableId>,
    ) {
        world.resource_mut::<AreaLaunches>().0.push(AreaLaunch {
            id,
            by,
            pos,
            unit_type,
            aimed,
        });
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_common::{Tick, Ticks};
    use campfire_math::Num;
    use campfire_sim::IdAllocator;

    use crate::areas::Areas;
    use crate::areas::area::Area;
    use crate::areas::area_spec::{AreaSpec, Inside};
    use crate::deliveries::delivering::Delivering;
    use crate::units::action_id::ActionId;
    use crate::units::by_type::ByType;
    use crate::units::filter::Filter;
    use crate::units::unit_type::UnitType;
    use crate::values::rank::Rank;
    use crate::values::relation_set::RelationSet;

    impl Areas {
        /// An area unit of `unit_type`, a type that reaches `radius`, as a client draws it: the
        /// type's spec in the match's book of them, and the area, from tick 0 to tick 1.
        pub fn standing(world: &mut World, unit_type: UnitType, radius: Num) -> Area {
            let spec = AreaSpec {
                radius,
                delay: Ticks::new(0),
                duration: Ticks::new(1),
                affects: Filter::of_relations(RelationSet::All),
                inside: Inside::default(),
            };
            world
                .get_resource_or_init::<ByType<AreaSpec>>()
                .set(unit_type, spec);
            let by = Delivering {
                source: IdAllocator::default().allocate(),
                action: ActionId::new(0),
                rank: Rank::FIRST,
                start: None,
                launch: None,
            };
            Area::new(by, None, Tick::new(1))
        }
    }
}

#[cfg(test)]
mod tests;
