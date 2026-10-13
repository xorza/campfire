use bevy_ecs::entity::Entity;
use bevy_ecs::observer::On;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_sim::{Keyed, Ordered, Position, StableId};

use crate::actions::shot_hits::ShotHits;
use crate::actions::targets::{TargetKey, Targets};
use crate::deliveries::Deliveries;
use crate::deliveries::teleported::Teleported;
use crate::geometry::fraction::Fraction;
use crate::projectiles::flight_state::FlightState;
use crate::projectiles::flights::{Aloft, Flights};
use crate::projectiles::projectile::{Flight, Projectile};
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::projectiles::struck_units::StruckUnits;
use crate::stats::life::Life;
use crate::units::body_grid::BodyGrid;
use crate::units::by_type::ByType;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;

/// The flight of the projectiles, a step each tick, and what their hits and ends do.
#[derive(Debug)]
pub(super) struct Flying;

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
            Without<Life>,
        >,
        (mut order, mut met, mut flying): (
            Local<'_, Ordered>,
            Local<'_, Vec<(Fraction, StableId)>>,
            Local<'_, Vec<StableId>>,
        ),
        mut grid: Local<'_, BodyGrid<TargetKey>>,
    ) {
        flying.clear();
        let lines = projectiles
            .iter()
            .any(|(.., projectile, _, _, _)| matches!(projectile.flight(), Flight::Line { .. }));
        if lines {
            grid.rebuild(targets.placed());
        }
        let mut flights = Flights {
            hits: &mut hits,
            deliveries: &mut deliveries,
            struck: &mut struck,
            grid: &grid,
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
