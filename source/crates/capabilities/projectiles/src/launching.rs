use bevy_ecs::system::{Query, Res, ResMut};
use campfire_actions::{ActionTarget, ProjectileFlight, Shots, Targets};
use campfire_declare::ByType;
use campfire_deliveries::{DeliverySpawner, NewDelivery};
use campfire_math::Vec3;
use campfire_sim::{EntityIndex, StableId};
use campfire_units::Body;

use crate::flight_state::FlightState;
use crate::launches::{Launch, LaunchPayload, Launches};
use crate::projectile::{CurveAim, Flight, Payload, Projectile};
use crate::projectile_spec::ProjectileSpec;

/// The tick's shots and launches, which spawn as projectiles.
#[derive(Debug)]
pub struct Launching;

impl Launching {
    /// Makes each of the tick's shots a launch: homing on its target from where its attacker
    /// stood, or for a type that flies a curve, on a curve from its attacker's center to its
    /// target's, none when its target no longer lives, or to the point it scattered to
    /// (04-capabilities/actions.md, A projectile may fly a curve).
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
            let curve = |to: Vec3, aimed: Option<StableId>| {
                let body = index
                    .get(shot.source)
                    .and_then(|source| bodies.get(source).ok());
                let from = Body::center(body, shot.from, map);
                Flight::curve(CurveAim { from, to, aimed }, spec, map)
            };
            let flight = match (spec.flight(), shot.target) {
                (ProjectileFlight::Curve, ActionTarget::Unit(target)) => {
                    let Some(volume) = targets.volume_of(target) else {
                        continue;
                    };
                    curve(volume.center(), Some(target))
                }
                (ProjectileFlight::Curve, ActionTarget::Point(at)) => curve(map.point(at), None),
                (ProjectileFlight::Homing | ProjectileFlight::Line, ActionTarget::Unit(target)) => {
                    Flight::homing(target)
                }
                _ => panic!("a curve's shot aims at a unit or a point, a homing one at a unit"),
            };
            // A curve launches from its first point, its attacker's center.
            let from = match flight {
                Flight::Curve { from, .. } => map.position(from),
                Flight::Homing { .. } | Flight::Line { .. } => shot.from,
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
    pub(super) fn launch(mut spawner: DeliverySpawner<'_, '_>, mut launches: ResMut<'_, Launches>) {
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
                (projectile, FlightState::of(flight))
            });
        }
        launches.clear();
    }
}
