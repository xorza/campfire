use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_actions::{ActionProgress, AttackTargets, Targets};
use campfire_common::Tick;
use campfire_declarations::ById;
use campfire_sim::{DueIndex, DueUnit, Position};
use campfire_units::{Dead, Facing};
use campfire_values::UnitType;

use crate::turret::Turret;
use crate::turret::turret_angle::TurretAngle;

/// The turrets' aim, a step each tick.
#[derive(Debug)]
pub(crate) struct Aiming;

impl Aiming {
    /// Turns each living unit's turret, in place, toward the bearing of its attack target, when
    /// that target lives, by at most its turn a tick, and with none back toward its rest at half
    /// that (04-capabilities/combat.md, A unit type may have a turret). It visits the units with
    /// an attack target and the turrets not at rest; a dead unit's turret keeps its angle. Its
    /// owner's client turns its own units' turrets as the server does.
    pub(crate) fn aim(
        turrets: Res<'_, ById<UnitType, Turret>>,
        targets: Targets<'_, '_>,
        attackers: Res<'_, DueIndex<AttackTargets>>,
        mut units: Query<
            '_,
            '_,
            (
                &UnitType,
                &Position,
                &Facing,
                &ActionProgress,
                &mut TurretAngle,
                Has<Dead>,
            ),
        >,
        (mut turning, mut visits): (ResMut<'_, DueIndex<TurretAngle>>, Local<'_, Vec<DueUnit>>),
    ) {
        visits.clear();
        attackers.peek_due(Tick::ZERO, &mut visits);
        turning.peek_due(Tick::ZERO, &mut visits);
        visits.sort_unstable_by_key(|unit| unit.id);
        visits.dedup_by_key(|unit| unit.id);
        for &DueUnit { id, entity } in &*visits {
            // A unit with no turret, or gone.
            let Ok((&unit_type, position, &facing, progress, mut angle, dead)) =
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
            let aim = progress
                .attack_target()
                .and_then(|target| targets.target(target))
                .and_then(|target| TurretAngle::toward(facing, position.get(), target.position));
            let turned = match aim {
                Some(aim) => angle.turned(aim, turret.turn),
                None => angle.turned(TurretAngle::REST, turret.rest),
            };
            angle.set_if_neq(turned);
            if turned == TurretAngle::REST {
                turning.forget(entity);
            } else {
                turning.push(Tick::ZERO, id, entity);
            }
        }
    }
}
