use bevy_ecs::system::{Query, Res, ResMut};
use campfire_actions::{ActionTarget, ProjectileFlight, Shots, Targets};
use campfire_declarations::ByType;
use campfire_deliveries::{DeliverySpawner, NewDelivery};
use campfire_sim::{EntityIndex, StableId};
use campfire_units::Body;

use crate::flight_state::FlightState;
use crate::launches::{Launch, LaunchPayload, Launches};
use crate::projectile::{Flight, LaunchAim, Payload, Projectile};
use crate::projectile_spec::ProjectileSpec;

/// The tick's shots and launches, which spawn as projectiles.
#[derive(Debug)]
pub struct Launching;

impl Launching {
    /// Makes each of the tick's shots a launch: homing on its target from where its attacker
    /// stood, or for a type that flies a curve or is steered, from its attacker's center to its
    /// target's, none when its target no longer lives, or to the point it scattered to
    /// (04-capabilities/actions.md, A projectile may fly a curve; 12-missiles.md, A steered
    /// flight).
    pub(super) fn take_shots(
        (mut shots, mut launches): (ResMut<'_, Shots>, ResMut<'_, Launches>),
        specs: Res<'_, ByType<ProjectileSpec>>,
        targets: Targets<'_, '_>,
        index: Res<'_, EntityIndex>,
        bodies: Query<'_, '_, &Body>,
    ) {
        for shot in shots.drain() {
            let spec = specs
                .get(shot.unit_type)
                .expect("a shot's projectile type has a spec");
            let map = targets.map();
            // A curve and a steered projectile launch from their attacker's center to their
            // target's or their point.
            let aim = || {
                let (to, aimed) = match shot.target {
                    ActionTarget::Unit(target) => {
                        (targets.volume_of(target)?.center(), Some(target))
                    }
                    ActionTarget::Point(at) => (map.point(at), None),
                    ActionTarget::None => panic!("a shot aims at a unit or a point"),
                };
                let body = index
                    .get(shot.source)
                    .and_then(|source| bodies.get(source).ok());
                let from = Body::center(body, shot.from, map);
                Some(LaunchAim { from, to, aimed })
            };
            let (flight, from) = match spec.flight() {
                ProjectileFlight::Homing | ProjectileFlight::Line => {
                    let ActionTarget::Unit(target) = shot.target else {
                        panic!("a homing shot aims at a unit");
                    };
                    (Flight::homing(target), shot.from)
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
                source: shot.source,
                from,
                unit_type: shot.unit_type,
                flight,
                payload: LaunchPayload::Attack(shot.damage),
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
        mut launches: ResMut<'_, Launches>,
        specs: Res<'_, ByType<ProjectileSpec>>,
    ) {
        launches.sort_by_source();
        let mut group: Option<(u32, StableId)> = None;
        for &Launch {
            id,
            source,
            from,
            unit_type,
            flight,
            payload,
        } in &launches
        {
            let delivery = NewDelivery {
                source,
                unit_type,
                position: from,
                id,
            };
            spawner.spawn(delivery, |id| {
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
                    LaunchPayload::Attack(damage) => Payload::Attack(damage),
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
