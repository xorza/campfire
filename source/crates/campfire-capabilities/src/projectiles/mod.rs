use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};

use campfire_sim::{Keyed, Ordered, Position, StableId, StateRegistry};

use crate::actions::action::Aim;
use crate::actions::action_book::ActionBook;
use crate::actions::action_target::ActionTarget;
use crate::actions::fan::Fan;
use crate::actions::range::Range;
use crate::actions::targets::Targets;
use crate::combat::CombatSet;
use crate::combat::pass_queue::PassQueue;
use crate::combat::shots::Shots;
use crate::deliveries::delivering::Delivering;
use crate::deliveries::delivery_spawner::DeliverySpawner;
use crate::deliveries::{Deliveries, DeliverySet};
use crate::projectiles::flights::{Aloft, Flights};
use crate::projectiles::launches::{Launch, LaunchPayload, Launches};
use crate::projectiles::projectile::{Flight, Payload, Projectile};

use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::projectiles::projectiles_effect::{ProjectilesEffect, Toward};
use crate::projectiles::struck_units::StruckUnits;
use crate::stats::pools::Pools;
use crate::units::body_grid::BodyGrid;
use crate::units::by_type::ByType;

use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::bounds::Bounds;
use crate::values::metric::Metric;

pub(crate) mod flights;
pub(crate) mod launches;
pub(crate) mod projectile;
pub(crate) mod projectile_data;
pub(crate) mod projectile_spec;
pub(crate) mod projectiles_api;
pub(crate) mod projectiles_effect;
pub(crate) mod struck_units;

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
        world.insert_resource(StruckUnits::default());
        world.insert_resource(ByType::<ProjectileSpec>::default());
        schedule.add_systems((
            fly.in_set(DeliverySet::Fly),
            take_shots.in_set(CombatSet::Fire),
            launch.in_set(CombatSet::Launch),
        ));
        registry.register_component::<Projectile>();
        registry.register_resource::<StruckUnits>();
    }

    /// Applies `effect`: a launch this tick, its own cast, from the point of the map's bounds
    /// nearest where it says. A direction of no length launches nothing.
    fn apply(world: &mut World, effect: ProjectilesEffect) {
        let from = Bounds::of(world).clamp(effect.from);
        let flight = match effect.toward {
            Toward::Unit(target) => Flight::Homing {
                target,
                flown: Num::ZERO,
                lost: false,
            },
            Toward::Direction(direction) => {
                let Some(direction) = world.resource::<Metric>().direction(direction) else {
                    return;
                };
                let range = Projectiles::range(world, effect.by, effect.unit_type);
                Flight::Line {
                    direction,
                    flown: Num::ZERO,
                    range: Projectiles::reach(Bounds::of(world), range, from, direction),
                    aimed: None,
                }
            }
        };
        let id = Some(effect.id);
        Projectiles::push(world, effect.by, effect.unit_type, from, id, [flight]);
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
        let to_point = matches!(delivers.aim, Aim::Point { .. });
        let spec = *world
            .resource::<ByType<ProjectileSpec>>()
            .get(unit_type)
            .expect("a delivery's projectile type has a spec");
        if let (ActionTarget::Unit(unit), true) = (target, spec.homing) {
            let homing = Flight::Homing {
                target: unit,
                flown: Num::ZERO,
                lost: false,
            };
            Projectiles::push(world, by, unit_type, from, None, [homing]);
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
        let bounds = Bounds::of(world);
        let flights = (0..fan.count.get()).map(|at| {
            let direction = aim.rotated_y(fan.turn(at).sin_cos());
            Flight::Line {
                direction,
                flown: Num::ZERO,
                range: Projectiles::reach(bounds, range, from, direction),
                aimed: target.unit(),
            }
        });
        Projectiles::push(world, by, unit_type, from, None, flights);
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
    fn reach(bounds: Bounds, range: Option<Num>, from: Position, direction: Vec3) -> Num {
        let exit = bounds.exit(from, direction);
        range.map_or(exit, |range| range.min(exit))
    }

    /// Queues `flights` of `unit_type` from `from` as one cast of `by`; with `id`, one flight,
    /// which a script launched with the id it took.
    fn push(
        world: &mut World,
        by: Delivering,
        unit_type: UnitType,
        from: Position,
        id: Option<StableId>,
        flights: impl IntoIterator<Item = Flight>,
    ) {
        let mut launches = world.resource_mut::<Launches>();
        let cast = launches.cast();
        let before = launches.launches.len();
        launches
            .launches
            .extend(flights.into_iter().map(|flight| Launch {
                id,
                source: by.source,
                from,
                unit_type,
                flight,
                payload: LaunchPayload::Action {
                    action: by.action,
                    rank: by.rank,
                    start: by.start,
                    cast,
                },
            }));
        debug_assert!(
            id.is_none() || launches.launches.len() == before + 1,
            "a script's id is one flight's"
        );
    }
}

/// Flies each projectile a step, in the order of their stable ids, as `Flights::fly` says. An
/// attack's hit deals its damage; an action's hits and ends run its hooks, and an ended
/// projectile despawns after them. A line's hits are kept while it flies, and a cast's, for a
/// type that strikes a unit once a cast, while any of its projectiles flies.
fn fly(
    targets: Targets<'_, '_>,
    specs: Res<'_, ByType<ProjectileSpec>>,
    (mut queue, mut deliveries, mut struck): (
        ResMut<'_, PassQueue>,
        ResMut<'_, Deliveries>,
        ResMut<'_, StruckUnits>,
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
    mut grid: Local<'_, BodyGrid<()>>,
) {
    flying.clear();
    let lines = projectiles
        .iter()
        .any(|(.., projectile, _, _)| matches!(projectile.flight(), Flight::Line { .. }));
    if lines {
        grid.rebuild(targets.placed());
    }
    let mut flights = Flights {
        queue: &mut queue,
        deliveries: &mut deliveries,
        struck: &mut struck,
        grid: &grid,
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
        let by = aloft.strikes_by();
        if flights.fly(&targets, aloft) {
            flights.deliveries.ended.push(entity);
        } else {
            flying.push(by);
        }
    }
    flying.sort_unstable();
    flying.dedup();
    flights.struck.keep(&flying);
}

/// Makes each of the tick's shots a launch, homing on its target from where its attacker stood.
fn take_shots(mut shots: ResMut<'_, Shots>, mut launches: ResMut<'_, Launches>) {
    for shot in shots.0.drain(..) {
        launches.launches.push(Launch {
            id: None,
            source: shot.source,
            from: shot.from,
            unit_type: shot.unit_type,
            flight: Flight::Homing {
                target: shot.target,
                flown: Num::ZERO,
                lost: false,
            },
            payload: LaunchPayload::Attack {
                action: shot.action,
                rank: shot.rank,
                amount: shot.amount,
                kind: shot.kind,
                roll: shot.roll,
            },
        });
    }
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
        id,
        source,
        from,
        unit_type,
        flight,
        payload,
    } in &launches.launches
    {
        spawner.spawn(source, from, unit_type, id, |id| {
            let payload = match payload {
                LaunchPayload::Action {
                    action,
                    rank,
                    start,
                    cast,
                } => {
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
                        start,
                        group: first,
                    }
                }
                LaunchPayload::Attack {
                    action,
                    rank,
                    amount,
                    kind,
                    roll,
                } => Payload::Attack {
                    action,
                    rank,
                    amount,
                    kind,
                    roll,
                },
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
