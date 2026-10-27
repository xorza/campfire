//! How the import flies a projectile object a weapon fires (09-zero-hour.md, The import converts
//! shells, splash, scatter, pitch, turrets and slow weapons; 09-zero-hour.md, A missile converts to
//! a steered projectile).

use campfire_projectiles::{CruiseData, CurveData, ProjectileData, ProjectileHits, SteerData};
use campfire_values::FilterData;

use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::locomotor_ini::{Appearance, Locomotor, LocomotorIni};
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::format::ini::object_ini::fight_data::{IniMissile, IniShell};
use crate::zero_hour::format::ini::weapon_ini::IniWeapon;
use crate::zero_hour::game_time;
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::weapon_drop_cause::WeaponDropCause;

/// How a projectile object flies, as the import converts it: a shell, by its
/// `DumbProjectileBehavior`, or a missile, by its `MissileAIUpdate` and the locomotor that thrusts
/// it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ObjectFlight<'l> {
    Shell(IniShell),
    Missile {
        missile: IniMissile,
        locomotor: &'l Locomotor,
    },
}

impl<'l> ObjectFlight<'l> {
    /// How `object` flies: as a shell, of a `DumbProjectileBehavior`, or as a missile, of a
    /// `MissileAIUpdate` whose first locomotor of `locomotors` thrusts; none for another.
    pub(crate) fn of(object: &IniObject, locomotors: &'l LocomotorIni) -> Option<Self> {
        let fight = object.fight();
        if let Some(shell) = fight.shell {
            return Some(Self::Shell(shell));
        }
        let missile = fight.missile?;
        let first = object.locomotors().first()?;
        let thrusts = |locomotor: &&Locomotor| locomotor.appearance == Appearance::Thrust;
        let locomotor = locomotors.get(first).filter(thrusts)?;
        Some(Self::Missile { missile, locomotor })
    }

    /// The projectile `weapon` fires, of `object`, which flies so, twice its geometry's radius
    /// wide, detonating on what its `ProjectileCollidesWith` names: a shell at its `WeaponSpeed`
    /// on the curve of its behavior; a missile steered by its behavior and its locomotor
    /// (09-zero-hour.md, A missile converts to a steered projectile).
    /// Why the import drops the weapon, for a mask of what the engine's units are not, a missile
    /// of what the engine's steer does not hold, a number past what the engine holds, or a
    /// projectile the engine's type refuses.
    pub(crate) fn projectile(
        self,
        weapon: &IniWeapon,
        object: &IniObject,
        names: &RulesNames,
    ) -> Result<ProjectileData, WeaponDropCause> {
        let hits = hits(weapon, names)?;
        let number = |value: f32| game_float::to_num(value).map_err(|_| WeaponDropCause::BadNumber);
        let width = number(2.0 * object.geometry().major)?;
        let (missile, locomotor) = match self {
            Self::Missile { missile, locomotor } => (missile, locomotor),
            Self::Shell(flight) => {
                let life_ms = game_time::of_ms(flight.lifespan_ms)?;
                let life_ms = u32::try_from(life_ms).map_err(|_| WeaponDropCause::BadNumber)?;
                let curve = CurveData::new(
                    [number(flight.first_indent)?, number(flight.first_height)?],
                    [number(flight.second_indent)?, number(flight.second_height)?],
                    number(flight.follow)?,
                    Some(life_ms),
                )
                .map_err(|_| WeaponDropCause::BadNumber)?;
                return ProjectileData::builder(number(weapon.speed)?)
                    .width(width)
                    .hits(hits)
                    .curve(curve)
                    .build()
                    .map_err(|_| WeaponDropCause::BadNumber);
            }
        };
        if missile.ignition_ms > 0 {
            return Err(WeaponDropCause::Ignition);
        }
        if missile.detonate_on_no_fuel {
            return Err(WeaponDropCause::FuelBurst);
        }
        if missile.garrison_kill {
            return Err(WeaponDropCause::GarrisonKill);
        }
        if missile.fuel_ms == 0 {
            return Err(WeaponDropCause::NoFuel);
        }
        let fuel_ms = game_time::of_ms(missile.fuel_ms)?;
        if fuel_ms <= game_time::of_ms(1)? {
            return Err(WeaponDropCause::Sprayer);
        }
        let speed = if missile.use_weapon_speed {
            weapon.speed
        } else {
            locomotor.speed
        };
        let mut steer = SteerData::builder(
            number(locomotor.turn_rate)?,
            u32::try_from(fuel_ms).map_err(|_| WeaponDropCause::BadNumber)?,
        )
        .launch_speed(number(missile.initial_velocity)?)
        .acceleration(number(locomotor.acceleration)?)
        .straight(number(missile.straight)?)
        .lock(number(missile.lock)?)
        // A missile at a point locks on at half its distance (`MissileAIUpdate::doAttackState`).
        .point_lock(number(missile.lock / 2.0)?)
        .tracks(missile.follow);
        if locomotor.preferred_height > 0.0 {
            let cruise =
                CruiseData::new(number(locomotor.preferred_height)?, number(missile.dive)?);
            steer = steer.cruise(cruise);
        }
        let steer = steer.build().map_err(|_| WeaponDropCause::BadNumber)?;
        ProjectileData::builder(number(speed)?)
            .width(width)
            .hits(hits)
            .steer(steer)
            .build()
            .map_err(|_| WeaponDropCause::BadNumber)
    }
}

/// What a projectile of `weapon` detonates on, as its `ProjectileCollidesWith` names it; why the
/// import drops the weapon, for shrubbery, projectiles or missiles, which are no units of the
/// engine's.
fn hits(weapon: &IniWeapon, names: &RulesNames) -> Result<ProjectileHits, WeaponDropCause> {
    let collides = weapon.collides;
    if collides.shrubbery
        || collides.projectiles
        || collides.small_missiles
        || collides.ballistic_missiles
    {
        return Err(WeaponDropCause::Collides);
    }
    // As each player is a team of its own, a structure of no other player is an enemy's or a
    // neutral's, and one of its own an ally's; `ALLIES` and `ENEMIES` name the objects of
    // their relation alone, structures among them, and no neutral's.
    let filters = [
        (collides.allies, &names.allies),
        (collides.enemies, &names.hostiles),
        (collides.structures, &names.enemy_structures),
        (collides.controlled_structures, &names.allied_structures),
        (collides.walls, &names.walls),
    ];
    let filters: Vec<FilterData> = filters
        .into_iter()
        .filter(|&(holds, _)| holds)
        .map(|(_, filter)| filter.clone())
        .collect();
    Ok(if filters.is_empty() {
        ProjectileHits::Nothing
    } else {
        ProjectileHits::Units(filters)
    })
}

#[cfg(test)]
pub(crate) mod samples {
    use campfire_math::Num;
    use campfire_projectiles::{CurveData, ProjectileData, ProjectileHits};
    use campfire_values::FilterData;

    /// The projectile of the fixture's `TankShell`, fired at `speed` m/s by a weapon of
    /// `STRUCTURES WALLS`: 2 m wide, its inner points at 50% and 90% of the way, 10 m up, its end
    /// following its victim by 100 m/s, for 10 s. 90% is 90 ÷ 100 in `f32`, 0.89999997615…, the
    /// bits 15,099,494 · 2⁻²⁴ of a `Num`.
    pub(crate) fn shell_projectile(speed: i64) -> ProjectileData {
        let filters =
            ["enemies:structure", "all:wall"].map(|text| FilterData::parse(text).unwrap());
        let curve = CurveData::new(
            [Num::HALF, Num::int(10)],
            [Num::from_bits(15_099_494), Num::int(10)],
            Num::int(100),
            Some(10_000),
        )
        .unwrap();
        ProjectileData::builder(Num::int(speed))
            .width(Num::int(2))
            .hits(ProjectileHits::Units(filters.into()))
            .curve(curve)
            .build()
            .unwrap()
    }
}

#[cfg(test)]
mod tests;
