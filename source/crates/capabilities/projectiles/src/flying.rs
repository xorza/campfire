use bevy_ecs::entity::Entity;
use bevy_ecs::observer::On;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_actions::Targets;
use campfire_declare::ByType;
use campfire_deliveries::{DeliveryQueue, Died, Teleported};
use campfire_math::Num;
use campfire_sim::{Keyed, Ordered, Position, StableId};
use campfire_stats::Life;
use campfire_units::{BodyUnit, UnitsNear};
use campfire_values::{Team, UnitType};

use crate::flight_state::FlightState;
use crate::flights::{Aloft, Flights};
use crate::projectile::{Flight, Projectile};
use crate::projectile_spec::ProjectileSpec;
use crate::struck_units::StruckUnits;

/// The flight of the projectiles, a step each tick, and what their hits and ends do.
#[derive(Debug)]
pub struct Flying;

impl Flying {
    /// Makes each projectile that homes on the unit `teleported` names lose it.
    pub(super) fn lose_teleported(
        teleported: On<'_, '_, Teleported>,
        mut projectiles: Query<'_, '_, (&Projectile, &mut FlightState)>,
    ) {
        Self::lose(teleported.id, &mut projectiles);
    }

    /// Makes each projectile that homes on the unit `died` names lose it.
    pub(super) fn lose_died(
        died: On<'_, '_, Died>,
        mut projectiles: Query<'_, '_, (&Projectile, &mut FlightState)>,
    ) {
        Self::lose(died.id, &mut projectiles);
    }

    fn lose(id: StableId, projectiles: &mut Query<'_, '_, (&Projectile, &mut FlightState)>) {
        for (projectile, mut state) in projectiles {
            if projectile.homes_on(id) {
                state.disjoint();
            }
        }
    }

    /// Flies each projectile a step, in the order of their stable ids, as `Flights::fly` says. Its
    /// hits and its end join the tick's deliveries, which combat answers for an attack's and
    /// whose hooks run for an action's, and an
    /// ended projectile despawns after them. A line's hits are kept while it flies, and a cast's,
    /// for a type that strikes a unit once a cast, while any of its projectiles flies.
    pub(super) fn fly(
        targets: Targets<'_, '_>,
        specs: Res<'_, ByType<ProjectileSpec>>,
        (mut deliveries, mut struck): (ResMut<'_, DeliveryQueue>, ResMut<'_, StruckUnits>),
        mut projectiles: Query<
            '_,
            '_,
            (
                Entity,
                &StableId,
                &mut Position,
                &Projectile,
                &mut FlightState,
                &UnitType,
                &Team,
            ),
            (Without<Life>, Without<BodyUnit>),
        >,
        (mut order, mut met, mut flying): (
            Local<'_, Ordered>,
            Local<'_, Vec<(Num, StableId)>>,
            Local<'_, Vec<StableId>>,
        ),
        near: UnitsNear<'_>,
    ) {
        flying.clear();
        // Only a line and a curve meet the units near.
        let meet = projectiles
            .iter()
            .any(|(.., projectile, _, _, _)| !matches!(projectile.flight(), Flight::Homing { .. }));
        let grid = meet.then(|| near.current());
        let mut flights = Flights {
            deliveries: &mut deliveries,
            struck: &mut struck,
            grid,
            met: &mut met,
        };
        let aloft = projectiles
            .iter()
            .map(|(entity, &id, ..)| Keyed { id, entity });
        for &Keyed { id, entity } in order.sort(aloft) {
            let (_, _, mut position, projectile, mut state, &unit_type, &team) = projectiles
                .get_mut(entity)
                .expect("a projectile in the order");
            let aloft = Aloft {
                id,
                team,
                spec: specs
                    .get(unit_type)
                    .expect("a projectile's type has a spec"),
                position: &mut position,
                projectile,
                state: &mut state,
            };
            let by = aloft.strikes_by();
            if flights.fly(&targets, aloft) {
                flights.deliveries.end(entity);
            } else {
                flying.push(by);
            }
        }
        flying.sort_unstable();
        flying.dedup();
        flights.struck.keep(&flying);
    }
}
