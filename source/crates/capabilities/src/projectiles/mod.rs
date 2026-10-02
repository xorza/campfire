use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_math::{Num, Tick, Vec3};

use campfire_sim::{Keyed, Ordered, Position, StableId, StateRegistry};

use crate::actions::action_book::{ActionBook, Aim, Fan};
use crate::actions::action_data::Range;
use crate::actions::action_slots::ActionTarget;
use crate::actions::targets::Targets;
use crate::combat::CombatSet;
use crate::combat::launches::{Launch, Launches};
use crate::combat::pass_queue::PassQueue;
use crate::deliveries::delivering::Delivering;
use crate::deliveries::delivery_spawner::DeliverySpawner;
use crate::deliveries::{Deliveries, DeliverySet};
use crate::projectiles::cast_hits::CastHits;
use crate::projectiles::flights::{Aloft, Flights};
use crate::projectiles::projectile::{Flight, Payload, Projectile};

use crate::projectiles::projectile_effect::{ProjectileEffect, Toward};
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::scripts::frame::Frame;
use crate::stats::pools::Pools;
use crate::units::by_type::ByType;

use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::bounds::Bounds;
use crate::values::metric::Metric;

pub(crate) mod cast_hits;
pub(crate) mod flights;
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
        Deliveries::install(world, schedule);
        world.insert_resource(Launches::default());
        world.insert_resource(CastHits::default());
        world.insert_resource(ByType::<ProjectileSpec>::default());
        schedule.add_systems((
            fly.in_set(DeliverySet::Fly),
            launch.in_set(CombatSet::Launch),
        ));
        registry.register_component::<Projectile>();
        registry.register_resource::<CastHits>();
    }

    /// Applies the next projectile the call in `frame` queued.
    pub(crate) fn apply_next(world: &mut World, frame: &mut Frame, _: Tick) {
        Projectiles::apply(world, frame.effects.take::<ProjectileEffect>());
    }

    /// Applies `effect`: a launch this tick, its own cast, from the point of the map's bounds
    /// nearest where it says. A direction of no length launches nothing.
    pub(crate) fn apply(world: &mut World, effect: ProjectileEffect) {
        let from = Bounds::of(world).clamp(effect.from);
        let flight = match effect.toward {
            Toward::Unit(target) => Flight::Homing {
                target,
                flown: Num::ZERO,
            },
            Toward::Direction(direction) => {
                let Some(direction) = direction.normalized() else {
                    return;
                };
                let range = Projectiles::range(world, effect.by, effect.unit_type);
                Flight::Line {
                    direction,
                    flown: Num::ZERO,
                    range: Projectiles::reach(world, range, from, direction),
                    aimed: None,
                }
            }
        };
        Projectiles::push(world, effect.by, effect.unit_type, from, &[flight]);
    }

    /// Launches `fan`, the delivery of `by`, which aimed at `target` from `from`: a homing
    /// projectile at a unit for a homing type, or the fan's projectiles along lines spread evenly
    /// over its spread around the aim, which for an action aimed at a point end there at the
    /// latest; nothing for no target, a unit that is gone, or an aim of no direction.
    pub(crate) fn deliver(
        world: &mut World,
        by: Delivering,
        from: Position,
        unit_type: UnitType,
        fan: Fan,
        target: ActionTarget,
    ) {
        let book = world.resource::<ActionBook>();
        let delivers = book.get(by.action).expect("a cast's action is in the book");
        let to_point = delivers.aim == Aim::Point;
        let spec = *world
            .resource::<ByType<ProjectileSpec>>()
            .get(unit_type)
            .expect("a delivery's projectile type has a spec");
        if let (ActionTarget::Unit(unit), true) = (target, spec.homing) {
            let homing = Flight::Homing {
                target: unit,
                flown: Num::ZERO,
            };
            Projectiles::push(world, by, unit_type, from, &[homing]);
            return;
        }
        let Some(at) = target.point(world) else {
            return;
        };
        let offset = world.resource::<Metric>().offset(from, at);
        let Some(aim) = offset.normalized() else {
            return;
        };
        let range = Projectiles::range(world, by, unit_type);
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
                    aimed: target.unit(),
                }
            })
            .collect();
        Projectiles::push(world, by, unit_type, from, &flights);
    }

    /// The range of a line projectile of `unit_type` of `by`: its type's, or else its action's
    /// at its rank; `None` for a global range.
    fn range(world: &World, by: Delivering, unit_type: UnitType) -> Option<Num> {
        let book = world.resource::<ActionBook>();
        let action = book
            .get(by.action)
            .expect("a delivery's action is in the book");
        let spec = world
            .resource::<ByType<ProjectileSpec>>()
            .get(unit_type)
            .expect("a delivery's projectile type has a spec");
        match (spec.range, action.values(by.rank).range) {
            (Some(range), _) | (None, Range::Meters(range)) => Some(range),
            (None, Range::Global) => None,
        }
    }

    /// How far a line of `range` flies from `from` along `direction`: up to the edge of the map's
    /// bounds, which a global range flies to.
    fn reach(world: &World, range: Option<Num>, from: Position, direction: Vec3) -> Num {
        let exit = Bounds::of(world).exit(from, direction);
        range.map_or(exit, |range| range.min(exit))
    }

    /// Queues `flights` of `unit_type` from `from` as one cast of `by`.
    fn push(
        world: &mut World,
        by: Delivering,
        unit_type: UnitType,
        from: Position,
        flights: &[Flight],
    ) {
        let mut launches = world.resource_mut::<Launches>();
        let cast = launches.cast();
        launches
            .launches
            .extend(flights.iter().map(|&flight| Launch {
                source: by.source,
                from,
                unit_type,
                flight,
                payload: Payload::Action {
                    action: by.action,
                    rank: by.rank,
                    group: by.source,
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
        ResMut<'_, PassQueue>,
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
        Local<'_, Ordered>,
        Local<'_, Vec<(u128, StableId)>>,
        Local<'_, Vec<StableId>>,
    ),
) {
    flying.clear();
    let mut flights = Flights {
        queue: &mut queue,
        deliveries: &mut deliveries,
        cast_hits: &mut cast_hits,
        met: &mut met,
    };
    let aloft = projectiles
        .iter()
        .map(|(entity, &id, ..)| Keyed { id, entity });
    for &Keyed { id, entity } in order.sort(aloft) {
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

/// Spawns the tick's launches, in the order of their source's stable id and then the order
/// launched, so each takes the same id in every run: a unit of its type, of its source's team
/// and player, with its type's tags. A cast's projectiles share the id of its first as their
/// group: a cast is one source's, and the stable sort keeps its launches together. A launch whose
/// source is gone launches nothing.
fn launch(mut spawner: DeliverySpawner<'_, '_>, mut launches: ResMut<'_, Launches>) {
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
        spawner.spawn(source, from, unit_type, |id| {
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
            Projectile::new(source, flight, payload)
                .expect("a launch flies within its range and carries what holds")
        });
    }
    launches.clear();
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::projectiles::Projectiles;
    use crate::projectiles::projectile_data::ProjectileData;
    use crate::projectiles::projectile_spec::ProjectileSpec;
    use crate::units::by_type::ByType;
    use crate::units::engine_tag::EngineTag;
    use crate::units::script_view::View;
    use crate::units::unit_type::UnitType;
    use bevy_ecs::world::World;
    use campfire_sim::TickRate;

    impl Projectiles {
        /// Makes `unit_type` a projectile type of `data`, tagged `projectile`, its speed a tick at
        /// the match's rate, which the package load checked: its `hits` filter names tags of the
        /// match's.
        pub(crate) fn load_type(world: &mut World, unit_type: UnitType, data: &ProjectileData) {
            let rate = *world.resource::<TickRate>();
            let view = world.non_send::<View>().clone();
            let spec = {
                let mut types = view.types_mut();
                types.give_tag(unit_type, EngineTag::Projectile.tag());
                ProjectileSpec::of(data, &types, rate)
            };
            world
                .resource_mut::<ByType<ProjectileSpec>>()
                .set(unit_type, spec);
        }
    }
}

#[cfg(test)]
mod tests;
