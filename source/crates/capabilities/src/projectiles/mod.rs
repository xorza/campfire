use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_script::rhai::Dynamic;
use std::iter;

use campfire_sim::{
    EntityIndex, IdAllocator, Position, SimSet, SimTick, StableId, StateRegistry, TickRate,
};

use crate::actions::action_book::{ActionBook, ActionId, Aim};
use crate::actions::action_data::Range;
use crate::actions::action_slots::ActionTarget;
use crate::combat::CombatSet;
use crate::combat::damage_queue::DamageQueue;
use crate::combat::launches::{Launch, Launches};
use crate::combat::targets::Targets;
use crate::projectiles::cast_hits::CastHits;
use crate::projectiles::deliveries::{Delivered, Deliveries};
use crate::projectiles::flights::{Aloft, Flights};
use crate::projectiles::hit_handle::HitHandle;
use crate::projectiles::projectile::{Flight, Payload, Projectile};
use crate::projectiles::projectile_data::ProjectileData;
use crate::projectiles::projectile_effect::{ProjectileEffect, Toward};
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;
use crate::stats::pools::Pools;
use crate::units::by_type::ByType;
use crate::units::filter::Filter;
use crate::units::owner::Owner;
use crate::units::script_view::View;
use crate::units::tag_book::TagBook;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::bounds::Bounds;
use crate::values::metric::Metric;

pub(crate) mod cast_hits;
pub(crate) mod deliveries;
pub(crate) mod flights;
pub(crate) mod hit;
pub(crate) mod hit_handle;
pub(crate) mod projectile;
pub(crate) mod projectile_data;
pub(crate) mod projectile_effect;
pub(crate) mod projectile_spec;
pub(crate) mod projectiles_api;

/// The `projectiles` capability: projectile units, which ranged attacks fire and actions
/// deliver. It builds on combat, which a match installs too.
#[derive(Debug)]
pub struct Projectiles;

impl Projectiles {
    /// Adds projectiles to a match. In Hit, before attacks strike, each projectile flies a step,
    /// and its hits and its end run their action's `on_hit` and `on_end`; after attacks strike
    /// and casts resolve, the tick's launches take off, and fly from the next tick.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        world.insert_resource(Launches::default());
        world.insert_resource(Deliveries::default());
        world.insert_resource(CastHits::default());
        world.insert_resource(ByType::<ProjectileSpec>::default());
        schedule.add_systems((
            (fly, deliver)
                .chain()
                .in_set(SimSet::Hit)
                .before(CombatSet::Strike),
            launch.in_set(CombatSet::Launch),
        ));
        registry.register_component::<Projectile>();
        registry.register_resource::<CastHits>();
    }

    /// Makes `unit_type` a projectile type of `data`, tagged `projectile`, its speed a tick at
    /// the match's rate, which the package load checked: the match declares the tag, and its
    /// `hits` filter names tags of the match's.
    pub fn load_type(world: &mut World, unit_type: UnitType, data: &ProjectileData) {
        let hz = world.resource::<TickRate>().hz().get();
        let view = world.non_send::<View>().clone();
        let hits = {
            let mut types = view.types_mut();
            let tag = types
                .declare(UnitTypeData::PROJECTILE_TAG)
                .expect("the match declared every tag its packages name");
            types.give_tag(unit_type, tag);
            match &data.hits {
                Some(filter) => Filter::resolve(filter, &types),
                None => Filter::parse("enemies", &types),
            }
        };
        let spec = ProjectileSpec {
            speed: data
                .speed
                .checked_div_int(i64::from(hz))
                .expect("a speed over a tick rate fits"),
            width: data.width,
            range: data.range,
            homing: data.homing,
            stop_on_hit: data.stop_on_hit,
            once_per_cast: data.once_per_cast,
            hits: hits.expect("the load checked the filter's tags"),
        };
        world
            .resource_mut::<ByType<ProjectileSpec>>()
            .set(unit_type, spec);
    }

    /// Applies `effect`: a launch this tick, its own cast, from the point of the map's bounds
    /// nearest where it says. A direction of no length launches nothing.
    pub(crate) fn apply(world: &mut World, effect: ProjectileEffect) {
        let from = Projectiles::bounds(world).clamp(effect.from);
        let flight = match effect.toward {
            Toward::Unit(target) => Flight::Homing {
                target,
                flown: Num::ZERO,
            },
            Toward::Direction(direction) => {
                let Some(direction) = direction.normalized() else {
                    return;
                };
                let range = Projectiles::range(world, effect.action, effect.rank);
                Flight::Line {
                    direction,
                    flown: Num::ZERO,
                    range: Projectiles::reach(world, range, from, direction),
                    aimed: None,
                }
            }
        };
        let (source, action, rank) = (effect.source, effect.action, effect.rank);
        Projectiles::push(world, source, from, action, rank, &[flight]);
    }

    /// Launches the delivery of `action` at `rank`, which `source` at `from` aimed at `target`:
    /// a homing projectile at a unit for a homing type, or its fan of projectiles along lines
    /// spread evenly over its spread around the aim, which for an action aimed at a point end
    /// there at the latest; nothing for an action that delivers at once, or an aim of no
    /// direction.
    pub(crate) fn deliver(
        world: &mut World,
        source: StableId,
        from: Position,
        action: ActionId,
        rank: u8,
        target: ActionTarget,
    ) {
        let book = world.resource::<ActionBook>();
        let delivers = book.get(action).expect("a cast's action is in the book");
        let (Some(fan), Some(unit_type)) = (delivers.fan, delivers.spawns) else {
            return;
        };
        let to_point = delivers.aim == Aim::Point;
        let spec = *world
            .resource::<ByType<ProjectileSpec>>()
            .get(unit_type)
            .expect("a delivery's projectile type has a spec");
        let (at, aimed) = match target {
            ActionTarget::None => return,
            ActionTarget::Unit(unit) if spec.homing => {
                let homing = Flight::Homing {
                    target: unit,
                    flown: Num::ZERO,
                };
                Projectiles::push(world, source, from, action, rank, &[homing]);
                return;
            }
            ActionTarget::Unit(unit) => {
                let index = world.resource::<EntityIndex>();
                let Some(&at) = index
                    .get(unit)
                    .and_then(|entity| world.get::<Position>(entity))
                else {
                    return;
                };
                (at, Some(unit))
            }
            ActionTarget::Point(at) => (at, None),
        };
        let offset = world.resource::<Metric>().offset(from, at);
        let Some(aim) = offset.normalized() else {
            return;
        };
        let range = Projectiles::range(world, action, rank);
        let distance = offset.length();
        let range = if to_point {
            Some(range.map_or(distance, |range| range.min(distance)))
        } else {
            range
        };
        let count = i64::from(fan.count.get());
        let degree = Num::PI / 180;
        let flights: Vec<Flight> = (0..count)
            .map(|at| {
                let turn = if count == 1 {
                    Num::ZERO
                } else {
                    fan.spread_deg * at / (count - 1) - fan.spread_deg / 2
                };
                let direction = aim.rotated_y((turn * degree).sin_cos());
                Flight::Line {
                    direction,
                    flown: Num::ZERO,
                    range: Projectiles::reach(world, range, from, direction),
                    aimed,
                }
            })
            .collect();
        Projectiles::push(world, source, from, action, rank, &flights);
    }

    /// The range of a line projectile of `action` at `rank`: its type's, or else the action's;
    /// `None` for a global range.
    fn range(world: &World, action: ActionId, rank: u8) -> Option<Num> {
        let book = world.resource::<ActionBook>();
        let action = book
            .get(action)
            .expect("a delivery's action is in the book");
        let unit_type = action.spawns.expect("a delivery binds its projectile type");
        let spec = world
            .resource::<ByType<ProjectileSpec>>()
            .get(unit_type)
            .expect("a delivery's projectile type has a spec");
        match (spec.range, action.values(rank).range) {
            (Some(range), _) | (None, Range::Meters(range)) => Some(range),
            (None, Range::Global) => None,
        }
    }

    /// How far a line of `range` flies from `from` along `direction`: up to the edge of the map's
    /// bounds, which a global range flies to.
    fn reach(world: &World, range: Option<Num>, from: Position, direction: Vec3) -> Num {
        let exit = Projectiles::bounds(world).exit(from, direction);
        range.map_or(exit, |range| range.min(exit))
    }

    /// The map's bounds, or the world's in a match without a map.
    fn bounds(world: &World) -> Bounds {
        world
            .get_resource::<Bounds>()
            .copied()
            .unwrap_or(Bounds::WORLD)
    }

    /// Queues `flights` from `from` as one cast of `source`'s `action` at `rank`.
    fn push(
        world: &mut World,
        source: StableId,
        from: Position,
        action: ActionId,
        rank: u8,
        flights: &[Flight],
    ) {
        let unit_type = world
            .resource::<ActionBook>()
            .get(action)
            .and_then(|action| action.spawns)
            .expect("a delivery binds its projectile type");
        let mut launches = world.resource_mut::<Launches>();
        let cast = launches.cast();
        launches
            .launches
            .extend(flights.iter().map(|&flight| Launch {
                source,
                from,
                unit_type,
                flight,
                payload: Payload::Action {
                    action,
                    rank,
                    group: source,
                },
                cast,
            }));
    }
}

/// Flies each projectile a step, in the order of their stable ids, as `Flights::fly` says. An
/// attack's hit deals its damage; an action's hits and ends run its hooks, and an ended
/// projectile despawns after them. A cast's hits are kept while any of its projectiles flies.
fn fly(
    targets: Targets<'_, '_>,
    specs: Res<'_, ByType<ProjectileSpec>>,
    (mut queue, mut deliveries, mut cast_hits): (
        ResMut<'_, DamageQueue>,
        ResMut<'_, Deliveries>,
        ResMut<'_, CastHits>,
    ),
    mut projectiles: Query<
        '_,
        '_,
        (
            Entity,
            &StableId,
            &mut Position,
            &mut Projectile,
            &UnitType,
            &Team,
        ),
        Without<Pools>,
    >,
    (mut order, mut met, mut flying): (
        Local<'_, Vec<(StableId, Entity)>>,
        Local<'_, Vec<(u128, StableId)>>,
        Local<'_, Vec<StableId>>,
    ),
) {
    order.clear();
    flying.clear();
    order.extend(projectiles.iter().map(|(entity, &id, ..)| (id, entity)));
    order.sort_unstable();
    let mut flights = Flights {
        queue: &mut queue,
        deliveries: &mut deliveries,
        cast_hits: &mut cast_hits,
        met: &mut met,
    };
    for &(id, entity) in &*order {
        let (_, _, mut position, mut projectile, &unit_type, &team) = projectiles
            .get_mut(entity)
            .expect("a projectile in the order");
        let aloft = Aloft {
            id,
            team,
            spec: *specs
                .get(unit_type)
                .expect("a projectile's type has a spec"),
            position: &mut position,
            projectile: &mut projectile,
        };
        if flights.fly(&targets, aloft) {
            flights.deliveries.ended.push(entity);
        } else if let Payload::Action { group, .. } = projectile.payload() {
            flying.push(group);
        }
    }
    flights.cast_hits.keep(|group| flying.contains(&group));
}

/// Runs the hooks of the tick's hits and ends, then despawns the projectiles that ended.
fn deliver(
    world: &mut World,
    (mut due, mut ended): (Local<'_, Vec<Delivered>>, Local<'_, Vec<Entity>>),
) {
    let mut deliveries = world.resource_mut::<Deliveries>();
    due.clear();
    due.append(&mut deliveries.delivered);
    ended.clear();
    ended.append(&mut deliveries.ended);
    run_hooks(world, &due);
    for &entity in &*ended {
        world.despawn(entity);
    }
}

/// Runs the hooks of `due`, in order: each with its action's params at its rank, its caster,
/// `()` once gone, its target, `()` for an end, and its hit, from its caster's player's pool, or
/// the think pool. A hook its action's script does not define does not run; a failed call
/// changes nothing and is recorded.
fn run_hooks(world: &mut World, due: &[Delivered]) {
    if due.is_empty() {
        return;
    }
    let Some(ctx) = world.get_non_send::<Ctx>().cloned() else {
        return;
    };
    let now = world.resource::<SimTick>().start();
    ScriptBatch::run(world, ctx.view(), |batch| {
        for delivered in due {
            let book = batch.world().resource::<ActionBook>();
            let action = book
                .get(delivered.action)
                .expect("a delivery's action is in the book");
            let Some(script) = action.hook(delivered.hook) else {
                continue;
            };
            let package = action.package;
            let view = ctx.view();
            let caster = view
                .unit(delivered.source)
                .map_or(Dynamic::UNIT, Dynamic::from);
            let target = delivered
                .reached
                .and_then(|id| view.unit(id))
                .map_or(Dynamic::UNIT, Dynamic::from);
            let owner = batch
                .world()
                .resource::<EntityIndex>()
                .get(delivered.source)
                .and_then(|entity| batch.world().get::<Owner>(entity))
                .map(|owner| owner.slot());
            let begun = ctx.frame().begin_cast(
                batch.world(),
                delivered.action,
                delivered.rank,
                delivered.source,
            );
            if let Err(error) = begun {
                batch.record(Some(delivered.source), delivered.hook, error);
                continue;
            }
            view.set_caller(package);
            let pool = owner.map_or(Pool::Think, Pool::Player);
            let hit = Dynamic::from(HitHandle::new(delivered.hit, view.clone()));
            let called = match delivered.hook {
                Hook::OnHit => batch.call(
                    pool,
                    script,
                    Hook::OnHit,
                    (ctx.clone(), caster, target, hit),
                ),
                _ => batch.call(pool, script, Hook::OnEnd, (ctx.clone(), caster, hit)),
            };
            match called {
                Ok(_) => ctx.apply(batch.world(), now),
                Err(error) => {
                    let error = CallError::from_script(error);
                    batch.record(Some(delivered.source), delivered.hook, error);
                }
            }
        }
    });
}

/// Spawns the tick's launches, in the order of their source's stable id and then the order
/// launched, so each takes the same id in every run: a unit of its type, of its source's team
/// and player, with its type's tags. A cast's projectiles share the id of its first as their
/// group: a cast is one source's, and the stable sort keeps its launches together. A launch whose
/// source is gone launches nothing.
fn launch(
    mut commands: Commands<'_, '_>,
    mut ids: ResMut<'_, IdAllocator>,
    mut launches: ResMut<'_, Launches>,
    (index, tag_book): (Res<'_, EntityIndex>, Option<Res<'_, TagBook>>),
    sources: Query<'_, '_, (&Team, Option<&Owner>)>,
) {
    launches.launches.sort_by_key(|launch| launch.source);
    let mut group: Option<(u32, StableId)> = None;
    for &Launch {
        source,
        from,
        unit_type,
        flight,
        payload,
        cast,
    } in &launches.launches
    {
        let Some((&team, owner)) = index
            .get(source)
            .and_then(|entity| sources.get(entity).ok())
        else {
            continue;
        };
        let id = ids.allocate();
        let payload = match payload {
            Payload::Action { action, rank, .. } => {
                let first = match group {
                    Some((at, first)) if at == cast => first,
                    _ => {
                        group = Some((cast, id));
                        id
                    }
                };
                Payload::Action {
                    action,
                    rank,
                    group: first,
                }
            }
            attack @ Payload::Attack { .. } => attack,
        };
        let projectile = Projectile::new(source, flight, payload)
            .expect("a launch flies within its range and carries what holds");
        let tags = tag_book.as_deref().map_or(UnitTags::default(), |book| {
            book.unit_tags(unit_type, iter::empty())
        });
        let mut unit = commands.spawn((id, from, unit_type, team, tags, projectile));
        if let Some(&owner) = owner {
            unit.insert(owner);
        }
    }
    launches.clear();
}

#[cfg(test)]
mod tests;
