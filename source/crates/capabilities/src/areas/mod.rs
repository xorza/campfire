use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::{
    Keyed, Ordered, Position, SimSet, SimTick, StableId, StateRegistry, TickRate, Ticks,
};

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionTarget;
use crate::areas::area::Area;
use crate::areas::area_data::AreaData;
use crate::areas::area_effect::AreaEffect;
use crate::areas::area_launches::{AreaLaunch, AreaLaunches};
use crate::areas::area_spec::{AreaSpec, Inside};
use crate::combat::CombatSet;
use crate::combat::targets::Targets;
use crate::deliveries::delivered::Delivered;
use crate::deliveries::delivering::Delivering;
use crate::deliveries::delivery_spawner::DeliverySpawner;
use crate::deliveries::hit::Hit;
use crate::deliveries::{Deliveries, DeliverySet};
use crate::scripts::hook::Hook;
use crate::stats::held_modifiers::{Held, HeldModifiers};
use crate::stats::{Stats, StatsSet};
use crate::units::by_type::ByType;
use crate::units::filter::Filter;
use crate::units::script_view::View;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::attitude::Attitude;
use crate::values::bounds::Bounds;

pub(crate) mod area;
pub(crate) mod area_data;
pub(crate) mod area_effect;
pub(crate) mod area_launches;
pub(crate) mod area_spec;
pub(crate) mod areas_api;

/// The `areas` capability: area units, which actions deliver. It builds on combat, which a match
/// installs too.
#[derive(Debug)]
pub struct Areas;

impl Areas {
    /// Adds areas to a match. In Hit, after projectiles fly, each area due triggers, and its
    /// reaches and its end run their action's `on_hit` and `on_end`; after the tick's projectiles
    /// launch, its areas land. In Resolve, before stats hold their modifiers, each area lists
    /// those it holds on the units inside.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        Deliveries::install(world, schedule);
        world.insert_resource(AreaLaunches::default());
        world.insert_resource(ByType::<AreaSpec>::default());
        schedule.add_systems((
            trigger.in_set(DeliverySet::Trigger),
            land.in_set(SimSet::Hit)
                .after(CombatSet::Launch)
                .before(CombatSet::Interval),
            hold_inside
                .in_set(SimSet::Resolve)
                .after(CombatSet::Die)
                .before(StatsSet::Hold),
        ));
        registry.register_component::<Area>();
    }

    /// Makes `unit_type` of `package` an area type of `data`, tagged `area`, its times in ticks
    /// at the match's rate, rounded up, which the package load checked: the match declares the
    /// tag, its `affects` filter names tags of the match's, and its `inside` modifiers of the
    /// package.
    pub fn load_type(world: &mut World, unit_type: UnitType, package: u16, data: &AreaData) {
        let rate = *world.resource::<TickRate>();
        let view = world.non_send::<View>().clone();
        let affects = {
            let mut types = view.types_mut();
            let tag = types
                .declare(UnitTypeData::AREA_TAG)
                .expect("the match declared every tag its packages name");
            types.give_tag(unit_type, tag);
            match &data.affects {
                Some(filter) => Filter::resolve(filter, &types),
                None => Filter::parse("enemies", &types),
            }
        };
        let modifier = |name: &Option<String>| {
            name.as_ref().map(|name| {
                Stats::modifier(world, package, name).expect("the load checked an area's modifiers")
            })
        };
        let ticks = |ms| rate.ticks(ms).expect("an area's time in ticks fits");
        let spec = AreaSpec {
            radius: data.radius,
            delay: ticks(data.delay_ms),
            duration: ticks(data.duration_ms),
            affects: affects.expect("the load checked the filter's tags"),
            inside: Inside {
                caster: modifier(&data.inside.caster),
                allies: modifier(&data.inside.allies),
                enemies: modifier(&data.inside.enemies),
            },
        };
        world
            .resource_mut::<ByType<AreaSpec>>()
            .set(unit_type, spec);
    }

    /// Applies `effect`: an area that lands this tick, at the point of the map's bounds nearest
    /// where it says.
    pub(crate) fn apply(world: &mut World, effect: AreaEffect) {
        let at = Bounds::of(world).clamp(effect.at);
        Areas::push(world, effect.by, at, None);
    }

    /// Lands the area of `by`, which aimed at `target` from `from`: on the point it aimed at,
    /// where the unit it aimed at stands, or at `from` for an action that aims at nothing;
    /// nothing for a unit that is gone.
    pub(crate) fn deliver(world: &mut World, by: Delivering, from: Position, target: ActionTarget) {
        let at = match target.point(world) {
            Some(at) => at,
            None if target == ActionTarget::None => from,
            None => return,
        };
        Areas::push(world, by, at, target.unit());
    }

    /// Queues an area of `by` to land at `at`, aimed at `aimed`.
    fn push(world: &mut World, by: Delivering, at: Position, aimed: Option<StableId>) {
        let unit_type = world
            .resource::<ActionBook>()
            .get(by.action)
            .and_then(|action| action.spawns)
            .expect("a delivery binds its area type");
        world.resource_mut::<AreaLaunches>().0.push(AreaLaunch {
            by,
            at,
            unit_type,
            aimed,
        });
    }
}

/// Triggers each area whose delay ended, and ends each whose time is up, in the order of their
/// stable ids. A trigger reaches each living unit its type's `affects` selects whose body comes
/// within its radius, whose tags block it as a target or not, by stable id, each with its
/// action's `on_hit`; an end runs `on_end`, and the area despawns after the hooks. Each hit is at
/// the area's centre, of no distance or direction.
fn trigger(
    targets: Targets<'_, '_>,
    (specs, tick): (Res<'_, ByType<AreaSpec>>, Res<'_, SimTick>),
    mut deliveries: ResMut<'_, Deliveries>,
    mut areas: Query<'_, '_, (Entity, &StableId, &Position, &Team, &UnitType, &mut Area)>,
    (mut order, mut reached): (Local<'_, Ordered>, Local<'_, Vec<StableId>>),
) {
    let now = tick.start();
    let placed = areas.iter().map(|(entity, &id, ..)| Keyed { id, entity });
    for &Keyed { id, entity } in order.sort(placed) {
        let (_, _, &pos, &team, &unit_type, mut area) =
            areas.get_mut(entity).expect("an area in the order");
        let hit = Hit {
            delivery: Some(id),
            target: area.aimed(),
            pos,
            distance: Num::ZERO,
            direction: Vec3::ZERO,
        };
        let by = area.by();
        let delivered = |hook, reached| Delivered {
            source: by.source,
            action: by.action,
            rank: by.rank,
            hook,
            reached,
            hit,
        };
        if area.triggers_at().is_some_and(|at| at <= now) {
            let spec = specs.get(unit_type).expect("an area's type has a spec");
            reached.clear();
            reached.extend(
                targets
                    .bodies()
                    .filter(|unit| {
                        let attitude = targets.attitude(team, unit.team);
                        spec.affects.selects(attitude, unit.tags)
                            && targets.reaches(pos, Num::ZERO, spec.radius, unit)
                    })
                    .map(|unit| unit.id),
            );
            reached.sort_unstable();
            let hits = reached
                .iter()
                .map(|&unit| delivered(Hook::OnHit, Some(unit)));
            deliveries.delivered.extend(hits);
            area.trigger();
        }
        if area.triggers_at().is_none() && area.ends_at() <= now {
            deliveries.delivered.push(delivered(Hook::OnEnd, None));
            deliveries.ended.push(entity);
        }
    }
}

/// Lands the tick's areas, in the order of their source's stable id and then the order queued,
/// so each takes the same id in every run: a unit of its type, of its source's team and player,
/// with its type's tags, which triggers its delay after this tick and one tick at least, and ends
/// at its trigger or after its duration, whichever is later. An area whose source is gone lands
/// nothing.
fn land(
    mut spawner: DeliverySpawner<'_, '_>,
    mut launches: ResMut<'_, AreaLaunches>,
    (specs, tick): (Res<'_, ByType<AreaSpec>>, Res<'_, SimTick>),
) {
    let now = tick.start();
    launches.0.sort_by_key(|launch| launch.by.source);
    for &AreaLaunch {
        by,
        at,
        unit_type,
        aimed,
    } in &launches.0
    {
        let spec = specs.get(unit_type).expect("an area's type has a spec");
        let triggers_at = now.after(spec.delay.max(Ticks::ONE));
        let ends_at = triggers_at.max(now.after(spec.duration));
        spawner.spawn(by.source, at, unit_type, |_| {
            Area::new(by, aimed, Some(triggers_at), ends_at)
                .expect("an area triggers before it ends")
        });
    }
    launches.0.clear();
}

/// Lists the modifiers each area holds this tick: its type's `inside` modifier for its source,
/// for the source's other allies and for the units that may be attacked, on each living unit
/// whose body is within its radius, whose tags block it as a target or not, from its source, by
/// its action at its rank.
fn hold_inside(
    targets: Targets<'_, '_>,
    specs: Res<'_, ByType<AreaSpec>>,
    mut held: ResMut<'_, HeldModifiers>,
    areas: Query<'_, '_, (&Position, &Team, &UnitType, &Area)>,
) {
    held.0.clear();
    for (&pos, &team, &unit_type, area) in &areas {
        let spec = specs.get(unit_type).expect("an area's type has a spec");
        if spec.inside == Inside::default() {
            continue;
        }
        for unit in targets.bodies() {
            if !targets.reaches(pos, Num::ZERO, spec.radius, &unit) {
                continue;
            }
            let by = area.by();
            let modifier = match targets.attitude(team, unit.team) {
                _ if unit.id == by.source => spec.inside.caster,
                Attitude::Friendly => spec.inside.allies,
                Attitude::Hostile | Attitude::Neutral => spec.inside.enemies,
            };
            if let Some(modifier) = modifier {
                held.0.push(Held {
                    target: unit.id,
                    modifier,
                    source: Some(by.source),
                    ability: Some(by.action),
                    rank: by.rank,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests;
