use bevy_ecs::entity::Entity;
use bevy_ecs::observer::On;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_actions::Targets;
use campfire_declarations::ById;
use campfire_deliveries::DeliveryQueue;
use campfire_math::Num;
use campfire_sim::{Keyed, Ordered, Position, StableId};
use campfire_units::{BodyUnit, Died, Life, Owner, Teleported, UnitsNear};
use campfire_values::{Team, UnitType};

use crate::flight_state::FlightState;
use crate::flights::{Aloft, TickFlights};
use crate::homers::Homers;
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
        homers: Res<'_, Homers>,
        mut states: Query<'_, '_, &mut FlightState>,
    ) {
        Self::lose(teleported.id, &homers, &mut states);
    }

    /// Makes each projectile that homes on the unit `died` names lose it.
    pub(super) fn lose_died(
        died: On<'_, '_, Died>,
        homers: Res<'_, Homers>,
        mut states: Query<'_, '_, &mut FlightState>,
    ) {
        Self::lose(died.id, &homers, &mut states);
    }

    fn lose(id: StableId, homers: &Homers, states: &mut Query<'_, '_, &mut FlightState>) {
        for projectile in homers.of(id) {
            states
                .get_mut(projectile)
                .expect("a projectile flies as it has a projectile's part")
                .disjoint();
        }
    }

    /// Flies each projectile a step, in the order of their stable ids, as `TickFlights::fly` says.
    /// Its hits and its end join the tick's deliveries, which combat answers for an attack's and
    /// whose hooks run for an action's, and an ended projectile despawns after them. A line's hits
    /// are kept while it flies, and a cast's, for a type that strikes a unit once a cast, while any
    /// of its projectiles flies.
    pub(super) fn fly(
        targets: Targets<'_, '_>,
        specs: Res<'_, ById<UnitType, ProjectileSpec>>,
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
                (&Team, Option<&Owner>),
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
        let mut flights = TickFlights {
            deliveries: &mut deliveries,
            struck: &mut struck,
            grid,
            met: &mut met,
        };
        let aloft = projectiles
            .iter()
            .map(|(entity, &id, ..)| Keyed { id, entity });
        for &Keyed { id, entity } in order.sort(aloft) {
            let (_, _, mut position, projectile, mut state, &unit_type, (&team, owner)) =
                projectiles
                    .get_mut(entity)
                    .expect("a projectile in the order");
            let aloft = Aloft {
                id,
                team,
                owner: owner.map(|owner| owner.player_slot()),
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
