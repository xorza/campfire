use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_actions::{ActionProgress, AttackTargets, Targets};
use campfire_common::Tick;
use campfire_declarations::ById;
use campfire_math::{Num, Vec3};
use campfire_sim::{DueIndex, DueUnit, Position};
use campfire_units::{Body, Dead, Facing, MapBook};
use campfire_values::UnitType;

use crate::turret::Turret;
use crate::turret::turret_aim::TurretAim;

/// The turrets' aim, a step each tick.
#[derive(Debug)]
pub(crate) struct Aiming;

impl Aiming {
    /// Turns each living unit's turret, in place, toward its attack target, when that target
    /// lives, by at most its turns a tick, and with none back toward its rest at half those
    /// (04-capabilities/combat.md, A unit type may have a turret; 04-capabilities/combat.md, A
    /// turret may pitch). It visits the units with an attack target and the turrets not at rest;
    /// a dead unit's turret keeps its aim. Its owner's client turns its own units' turrets as the
    /// server does.
    pub(crate) fn aim(
        turrets: Res<'_, ById<UnitType, Turret>>,
        targets: Targets<'_, '_>,
        attackers: Res<'_, DueIndex<AttackTargets>>,
        mut units: Query<
            '_,
            '_,
            (
                (&UnitType, &Position, Option<&Body>),
                &Facing,
                &ActionProgress,
                &mut TurretAim,
                Has<Dead>,
            ),
        >,
        (mut turning, mut visits): (ResMut<'_, DueIndex<TurretAim>>, Local<'_, Vec<DueUnit>>),
    ) {
        visits.clear();
        attackers.peek_due(Tick::ZERO, &mut visits);
        turning.peek_due(Tick::ZERO, &mut visits);
        visits.sort_unstable_by_key(|unit| unit.id);
        visits.dedup_by_key(|unit| unit.id);
        for &DueUnit { id, entity } in &*visits {
            // A unit with no turret, or gone.
            let Ok(((&unit_type, position, body), &facing, progress, mut aim, dead)) =
                units.get_mut(entity)
            else {
                turning.forget(entity);
                continue;
            };
            if dead {
                continue;
            }
            let turret = turrets
                .get(unit_type)
                .expect("a turret's unit is of a type with one");
            let toward = progress
                .attack_target()
                .and_then(|target| targets.target(target))
                .and_then(|target| {
                    let map = targets.map();
                    let top = Self::top(body, position.get(), map);
                    TurretAim::toward(turret, facing, top, map.point(target.position))
                });
            let turned = match toward {
                Some(toward) => aim.aimed(toward, turret),
                None => aim.rested(turret),
            };
            aim.set_if_neq(turned);
            if turned == TurretAim::rest(turret) {
                turning.forget(entity);
            } else {
                turning.push(Tick::ZERO, id, entity);
            }
        }
    }

    /// The top of a unit with `body` at `position`, on the map of `map`: the position's point in
    /// space raised by its height, which its turret pitches from.
    pub(crate) fn top(body: Option<&Body>, position: Vec3, map: &MapBook) -> Vec3 {
        let height = body.map_or(Num::ZERO, |body| body.height());
        map.point(position) + Vec3::new(Num::ZERO, height, Num::ZERO)
    }
}
