use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::lifecycle::Remove;
use bevy_ecs::observer::On;
use bevy_ecs::query::{Has, With};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_actions::{ActionProgress, ActionSlots, AttackTargets, Targets};
use campfire_common::Tick;
use campfire_declarations::ById;
use campfire_math::{Num, Vec3};
use campfire_sim::{DueIndex, DueUnit, Position, StableId};
use campfire_units::{Body, Dead, Facing, MapBook};
use campfire_values::{Team, UnitType};

use crate::turret::Turret;
use crate::turret::turret_aim::TurretAim;

/// The turrets' aim, a step each tick.
#[derive(Debug)]
pub(crate) struct Aiming;

impl Aiming {
    /// Puts the unit whose `Dead` goes back into the turning, as it lives again, its turret
    /// where its death held it: an observer of `Dead`'s removal.
    pub(crate) fn revived(
        removed: On<'_, '_, Remove<Dead>>,
        turrets: Query<'_, '_, &StableId, With<TurretAim>>,
        turning: Option<ResMut<'_, DueIndex<TurretAim>>>,
    ) {
        let entity = removed.entity;
        if let (Ok(&id), Some(mut turning)) = (turrets.get(entity), turning) {
            turning.push(Tick::ZERO, id, entity);
        }
    }

    /// Turns each living unit's turret, in place, toward its attack target, when that target is a
    /// living enemy, as the unit's attack takes it, and the weapon combat chose against it is of a
    /// slot the turret carries, by at most its turns a tick, and else back toward its rest at half
    /// those, as Zero Hour's turret aims only for the weapon slots it controls
    /// (`AIUpdateInterface::getWhichTurretForCurrentWeapon`; 04-capabilities/combat.md, A unit type
    /// may have a turret; 04-capabilities/combat.md, A turret may pitch). It visits the units with
    /// an attack target and the turrets not at rest; a dead unit's turret keeps its aim. Its
    /// owner's client turns its own units' turrets as the server does.
    pub(crate) fn aim(
        turrets: Res<'_, ById<UnitType, Turret>>,
        targets: Targets<'_, '_>,
        attackers: Res<'_, DueIndex<AttackTargets>>,
        mut units: Query<
            '_,
            '_,
            (
                (&UnitType, &Position, Option<&Body>),
                (&Team, &ActionSlots),
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
            let Ok((
                (&unit_type, position, body),
                (&team, slots),
                &facing,
                progress,
                mut aim,
                dead,
            )) = units.get_mut(entity)
            else {
                turning.forget(entity);
                continue;
            };
            // A dead unit's turret holds still: it leaves the turning, which its revival puts it
            // back in.
            if dead {
                turning.forget(entity);
                continue;
            }
            let turret = turrets
                .get(unit_type)
                .expect("a turret's unit is of a type with one");
            let toward = progress
                .aimed_at()
                .and_then(|target| targets.enemy(team, target))
                .filter(|_| {
                    let slot = progress.weapon();
                    slot.is_some_and(|slot| turret.carries(slots, slot))
                })
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
