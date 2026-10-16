use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Query, Res};
use campfire_actions::{ActionProgress, Targets};
use campfire_declare::ByType;
use campfire_sim::Position;
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
    /// that (04-capabilities/combat.md, A unit type may have a turret). Its owner's client turns
    /// its own units' turrets as the server does.
    pub(crate) fn aim(
        turrets: Res<'_, ByType<Turret>>,
        targets: Targets<'_, '_>,
        mut units: Query<
            '_,
            '_,
            (
                &UnitType,
                &Position,
                &Facing,
                &ActionProgress,
                &mut TurretAngle,
            ),
            Without<Dead>,
        >,
    ) {
        for (&unit_type, position, &facing, progress, mut angle) in &mut units {
            let turret = turrets
                .get(unit_type)
                .expect("a turret's unit is of a type with one");
            let aim = progress
                .attack_target()
                .and_then(|target| targets.living(target))
                .and_then(|target| TurretAngle::toward(facing, position.get(), target.pos));
            let turned = match aim {
                Some(aim) => angle.turned(aim, turret.turn),
                None => angle.turned(TurretAngle::REST, turret.rest),
            };
            angle.set_if_neq(turned);
        }
    }
}
