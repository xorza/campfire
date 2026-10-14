use bevy_ecs::entity::Entity;
use bevy_ecs::observer::On;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_actions::{ShotHits, Targets};
use campfire_declare::ByType;
use campfire_deliveries::{Deliveries, Teleported};
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
        for (projectile, mut state) in &mut projectiles {
            if projectile.homes_on(teleported.id) {
                state.disjoint();
            }
        }
    }

    /// Flies each projectile a step, in the order of their stable ids, as `Flights::fly` says. An
    /// attack's hit joins the tick's shot hits; an action's hits and ends run its hooks, and an
    /// ended projectile despawns after them. A line's hits are kept while it flies, and a cast's,
    /// for a type that strikes a unit once a cast, while any of its projectiles flies.
    pub(super) fn fly(
        targets: Targets<'_, '_>,
        specs: Res<'_, ByType<ProjectileSpec>>,
        (mut hits, mut deliveries, mut struck): (
            ResMut<'_, ShotHits>,
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
        mut near: UnitsNear<'_, '_>,
    ) {
        flying.clear();
        let lines = projectiles
            .iter()
            .any(|(.., projectile, _, _, _)| matches!(projectile.flight(), Flight::Line { .. }));
        let grid = lines.then(|| near.current());
        let mut flights = Flights {
            hits: &mut hits,
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
                spec: *specs
                    .get(unit_type)
                    .expect("a projectile's type has a spec"),
                position: &mut position,
                projectile,
                state: &mut state,
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
}
