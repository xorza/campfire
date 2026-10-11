use bevy_ecs::system::ResMut;
use campfire_sim::StableId;

use crate::combat::shots::Shots;
use crate::deliveries::delivery_spawner::DeliverySpawner;
use crate::projectiles::flight_state::FlightState;
use crate::projectiles::launches::{Launch, LaunchPayload, Launches};
use crate::projectiles::projectile::{Flight, Payload, Projectile};

/// The tick's shots and launches, which spawn as projectiles.
#[derive(Debug)]
pub(super) struct Launching;

impl Launching {
    /// Makes each of the tick's shots a launch, homing on its target from where its attacker stood.
    pub(super) fn take_shots(mut shots: ResMut<'_, Shots>, mut launches: ResMut<'_, Launches>) {
        for shot in shots.0.drain(..) {
            launches.push(Launch {
                id: None,
                source: shot.source,
                from: shot.from,
                unit_type: shot.unit_type,
                flight: Flight::homing(shot.target),
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
        } in launches.iter()
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
                let projectile = Projectile::new(source, flight, payload)
                    .expect("a launch flies a range and carries what holds");
                (projectile, FlightState::START)
            });
        }
        launches.clear();
    }
}
