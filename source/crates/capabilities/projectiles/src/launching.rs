use bevy_ecs::system::{Query, Res, ResMut};
use campfire_actions::{ActionTarget, Shots, Targets};
use campfire_declarations::ById;
use campfire_deliveries::{DeliverySpawner, NewDelivery};
use campfire_sim::{EntityIndex, StableId};
use campfire_units::Body;

use crate::flight_state::FlightState;
use crate::projectile::{Flight, LaunchAim, Payload, Projectile};
use crate::projectile_flight::ProjectileFlight;
use crate::projectile_launches::{Launch, LaunchPayload, ProjectileLaunches};
use crate::projectile_spec::ProjectileSpec;
use campfire_values::UnitType;

/// The tick's shots and launches, which spawn as projectiles.
#[derive(Debug)]
pub struct Launching;

impl Launching {
    /// Makes each of the tick's shots a launch, from its barrel when it leaves one, else homing on
    /// its target from where its attacker stood, or for a type that flies a curve or is steered,
    /// from its attacker's center: at its target's center, none when its target no longer lives,
    /// or at the point it scattered to (04-capabilities/actions.md, A projectile may fly a curve;
    /// 04-capabilities/actions.md, A projectile may be steered; 04-capabilities/combat.md, A shot
    /// leaves its weapon's barrel).
    pub(super) fn take_shots(
        (mut shots, mut launches): (ResMut<'_, Shots>, ResMut<'_, ProjectileLaunches>),
        specs: Res<'_, ById<UnitType, ProjectileSpec>>,
        targets: Targets<'_, '_>,
        index: Res<'_, EntityIndex>,
        bodies: Query<'_, '_, &Body>,
    ) {
        for shot in shots.drain() {
            let spec = specs
                .get(shot.unit_type)
                .expect("a shot's projectile type has a spec");
            let map = targets.map();
            // A curve and a steered projectile launch from their barrel or their attacker's
            // center to their target's or their point.
            let aim = || {
                let (to, aimed) = match shot.target {
                    ActionTarget::Unit(target) => {
                        (targets.volume_of(target)?.center(), Some(target))
                    }
                    ActionTarget::Point(at) => (map.point(at), None),
                    ActionTarget::None => panic!("a shot aims at a unit or a point"),
                };
                let from = shot.launch.map_or_else(
                    || {
                        let body = index
                            .get(shot.source)
                            .and_then(|source| bodies.get(source).ok());
                        Body::center(body, shot.from, map)
                    },
                    |launch| launch.at,
                );
                let way = shot.launch.map(|launch| launch.way);
                Some(LaunchAim {
                    from,
                    to,
                    way,
                    aimed,
                })
            };
            let (flight, from) = match spec.flight() {
                ProjectileFlight::Homing | ProjectileFlight::Line => {
                    let ActionTarget::Unit(target) = shot.target else {
                        panic!("a homing shot aims at a unit");
                    };
                    let from = shot
                        .launch
                        .map_or(shot.from, |launch| map.position(launch.at));
                    (Flight::homing(target), from)
                }
                ProjectileFlight::Curve | ProjectileFlight::Steered => {
                    let Some(aim) = aim() else {
                        continue;
                    };
                    (Flight::aimed(aim, spec, map), map.position(aim.from))
                }
            };
            launches.push(Launch {
                id: None,
                from,
                unit_type: shot.unit_type,
                flight,
                payload: LaunchPayload::Attack {
                    source: shot.source,
                    damage: shot.damage,
                },
            });
        }
    }

    /// Spawns the tick's launches, in the order of their source's stable id and then the order
    /// launched, so each takes the same id in every run: a unit of its type, of its source's team
    /// and player, with its type's tags. A cast's projectiles share the id of its first as their
    /// group: a cast is one source's, and the stable sort keeps its launches together. A launch
    /// whose source is gone launches nothing.
    pub(super) fn launch(
        mut spawner: DeliverySpawner<'_, '_>,
        mut launches: ResMut<'_, ProjectileLaunches>,
        specs: Res<'_, ById<UnitType, ProjectileSpec>>,
    ) {
        launches.sort_by_source();
        let mut group: Option<(u32, StableId)> = None;
        for launch in &launches {
            let &Launch {
                id,
                from,
                unit_type,
                flight,
                payload,
            } = launch;
            let source = launch.source();
            let delivery = NewDelivery {
                source,
                unit_type,
                position: from,
                id,
            };
            spawner.spawn(delivery, |id| {
                let payload = match payload {
                    LaunchPayload::Action { by, cast } => {
                        let first = match group {
                            Some((at, first)) if at == cast => first,
                            _ => {
                                group = Some((cast, id));
                                id
                            }
                        };
                        Payload::Action {
                            action: by.action,
                            rank: by.rank,
                            start: by.start,
                            launch: by.launch,
                            group: first,
                        }
                    }
                    LaunchPayload::Attack { damage, .. } => Payload::Attack(damage),
                };
                let projectile = Projectile::new(source, flight, payload)
                    .expect("a launch flies a range and carries what holds");
                let spec = specs.get(unit_type).expect("a launch's type has a spec");
                (projectile, FlightState::of(flight, spec))
            });
        }
        launches.clear();
    }
}
