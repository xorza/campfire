use std::collections::BTreeMap;

use crate::zero_hour::error::IniError;
use crate::zero_hour::ini_text::{IniLine, IniText};

/// The weapons of Zero Hour's `Weapon.ini` files, as `WeaponStore` reads them, by their names, of
/// their case.
#[derive(Debug, Default)]
pub(crate) struct WeaponIni {
    weapons: BTreeMap<String, IniWeapon>,
}

/// What the import reads of a weapon (`WeaponTemplate`), each the template's default when absent:
/// its `PrimaryDamage`, `PrimaryDamageRadius`, `SecondaryDamageRadius`, `ScatterRadius`,
/// `ScatterRadiusVsInfantry`, `AttackRange` and `MinimumAttackRange`; its `WeaponSpeed` in units
/// a second, `INSTANT` by default; whether it names a `LaserName`; whether it may hit the ground,
/// `AntiGround`, yes by default; whether its `RadiusDamageAffects` holds `SUICIDE`; whether it
/// deals its damage at its own place, `DamageDealtAtSelfPosition`; whether it fires faster as it
/// keeps firing, of a `ContinuousFireOne` or a `ContinuousFireTwo`; its `DamageType` as the INI
/// writes it, none for the default `EXPLOSION`; its `DelayBetweenShots`, `ClipSize`,
/// `ClipReloadTime` and `PreAttackDelay`, times in milliseconds; and its `ProjectileObject`, none
/// for `NONE`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IniWeapon {
    pub(crate) damage: f32,
    pub(crate) radius: f32,
    pub(crate) secondary_radius: f32,
    pub(crate) scatter: f32,
    pub(crate) infantry_scatter: f32,
    pub(crate) range: f32,
    pub(crate) speed: f32,
    pub(crate) laser: bool,
    pub(crate) anti_ground: bool,
    pub(crate) suicide: bool,
    pub(crate) at_self: bool,
    pub(crate) continuous_fire: bool,
    pub(crate) min_range: f32,
    pub(crate) damage_type: Option<String>,
    pub(crate) delay: ShotDelay,
    pub(crate) clip_size: i32,
    pub(crate) clip_reload_ms: i32,
    pub(crate) pre_attack_ms: i32,
    pub(crate) projectile: Option<String>,
}

/// A weapon's `DelayBetweenShots`: one time in milliseconds, or a range of `Min:` and `Max:`, of
/// which the game draws one for each shot (`WeaponTemplate::parseShotDelay`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShotDelay {
    Fixed(i32),
    Random,
}

impl Default for IniWeapon {
    fn default() -> IniWeapon {
        IniWeapon {
            damage: 0.0,
            radius: 0.0,
            secondary_radius: 0.0,
            scatter: 0.0,
            infantry_scatter: 0.0,
            range: 0.0,
            speed: IniWeapon::INSTANT,
            laser: false,
            anti_ground: true,
            suicide: false,
            at_self: false,
            continuous_fire: false,
            min_range: 0.0,
            damage_type: None,
            delay: ShotDelay::default(),
            clip_size: 0,
            clip_reload_ms: 0,
            pre_attack_ms: 0,
            projectile: None,
        }
    }
}

impl Default for ShotDelay {
    fn default() -> ShotDelay {
        ShotDelay::Fixed(0)
    }
}

impl WeaponIni {
    /// Reads one INI file, adding its weapons to those read before: one read again keeps its
    /// fields and takes the new ones.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        let text = IniText::of(bytes);
        let mut open: Option<String> = None;
        for line in text.lines() {
            let word = line.word();
            match &open {
                None if word == "weapon" => {
                    let name = *line
                        .tokens
                        .get(1)
                        .ok_or(IniError::NoName { line: line.number })?;
                    self.weapons.entry(name.to_owned()).or_default();
                    open = Some(name.to_owned());
                }
                None if word == "end" => return Err(IniError::StrayEnd { line: line.number }),
                None => return Err(IniError::NotBlock { line: line.number }),
                Some(_) if word == "end" => open = None,
                Some(name) => self
                    .weapons
                    .get_mut(name)
                    .expect("an open weapon is in the list")
                    .field(&word, &line)?,
            }
        }
        match open {
            None => Ok(()),
            Some(_) => Err(IniError::Unclosed),
        }
    }

    /// The weapon `name`; none for a name no file defines.
    pub(crate) fn get(&self, name: &str) -> Option<&IniWeapon> {
        self.weapons.get(name)
    }
}

impl IniWeapon {
    /// A template's default `WeaponSpeed`, 999,999 units a second, which the game holds
    /// "effectively instant".
    pub(crate) const INSTANT: f32 = 999_999.0;

    /// Takes the line of `word`, a field the import reads or another.
    fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        let none = |value: &str| (!value.eq_ignore_ascii_case("none")).then(|| value.to_owned());
        match word {
            "primarydamage" => self.damage = at.real()?,
            "primarydamageradius" => self.radius = at.real()?,
            "secondarydamageradius" => self.secondary_radius = at.real()?,
            "scatterradius" => self.scatter = at.real()?,
            "scatterradiusvsinfantry" => self.infantry_scatter = at.real()?,
            "attackrange" => self.range = at.real()?,
            "weaponspeed" => self.speed = at.real()?,
            "lasername" => self.laser = at.tokens.len() > 1,
            "antiground" => self.anti_ground = at.yes()?,
            "radiusdamageaffects" => self.affects(at),
            "damagedealtatselfposition" => self.at_self = at.yes()?,
            "continuousfireone" | "continuousfiretwo" => self.continuous_fire = true,
            "minimumattackrange" => self.min_range = at.real()?,
            "damagetype" => self.damage_type = Some(at.value()?.to_owned()),
            "delaybetweenshots" => {
                let value = at.value()?;
                self.delay = if value.to_ascii_lowercase().starts_with("min") {
                    ShotDelay::Random
                } else {
                    ShotDelay::Fixed(at.int()?)
                };
            }
            "clipsize" => self.clip_size = at.int()?,
            "clipreloadtime" => self.clip_reload_ms = at.int()?,
            "preattackdelay" => self.pre_attack_ms = at.int()?,
            "projectileobject" => self.projectile = none(at.value()?),
            _ => {}
        }
        Ok(())
    }

    /// Takes a `RadiusDamageAffects` line, of `SUICIDE` alone, as `INI::parseBitString32` reads
    /// it: `NONE` clears every flag, a list of flags sets those alone, and `+` and `-` add a flag
    /// to those held or take it away.
    fn affects(&mut self, at: &IniLine<'_>) {
        let suicide = |name: &str| name.eq_ignore_ascii_case("suicide");
        let mut listed = false;
        for token in &at.tokens[1..] {
            if token.eq_ignore_ascii_case("none") {
                self.suicide = false;
                break;
            }
            if let Some(name) = token.strip_prefix('+') {
                self.suicide |= suicide(name);
            } else if let Some(name) = token.strip_prefix('-') {
                self.suicide &= !suicide(name);
            } else {
                if !listed {
                    self.suicide = false;
                    listed = true;
                }
                self.suicide |= suicide(token);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod internals {
    /// `Data\INI\Weapon.ini` of the fixture install: a rifle that fires a clip of 3 at once, a
    /// cannon that fires a shell, a flamer of a radius, a knife that waits before it strikes, a
    /// mortar of a minimum range, a pistol of a random delay, a marker of no delay, a shotgun of a
    /// second radius, a gatling that scatters at infantry, a bolt of a set speed, which may delay its
    /// damage, a stinger that hits no ground, a blast that kills its own unit, a pulse dealt at
    /// its own place, a minigun that fires faster as it keeps firing, a disabler of subdual
    /// damage, a mender of healing damage, and a beam of the bolt's speed, a laser, which does not
    /// delay its damage.
    pub(crate) const WEAPONS: &str = "Weapon RangerRifle\r
  PrimaryDamage = 5.0\r
  PrimaryDamageRadius = 0.0\r
  AttackRange = 100.0\r
  DamageType = SMALL_ARMS\r
  DelayBetweenShots = 100 ; ms\r
  ClipSize = 3\r
  ClipReloadTime = 700\r
  ProjectileObject = NONE\r
End\r
Weapon TankCannon\r
  PrimaryDamage = 60\r
  AttackRange = 150\r
  DamageType = ARMOR_PIERCING\r
  DelayBetweenShots = 2000\r
  ProjectileObject = TankShell\r
End\r
Weapon Flamer\r
  PrimaryDamage = 10\r
  PrimaryDamageRadius = 5\r
  AttackRange = 60\r
  DamageType = FLAME\r
  DelayBetweenShots = 300\r
End\r
Weapon Knife\r
  PrimaryDamage = 30\r
  AttackRange = 10\r
  DamageType = MELEE\r
  DelayBetweenShots = 1000\r
  PreAttackDelay = 833\r
End\r
Weapon Mortar\r
  PrimaryDamage = 40\r
  AttackRange = 300\r
  MinimumAttackRange = 50\r
  DelayBetweenShots = 3000\r
End\r
Weapon Pistol\r
  PrimaryDamage = 4\r
  AttackRange = 90\r
  DamageType = small_arms\r
  DelayBetweenShots = Min:200 Max:400\r
End\r
Weapon Marker\r
  PrimaryDamage = 0\r
  AttackRange = 50\r
End\r
Weapon Shotgun\r
  PrimaryDamage = 20\r
  SecondaryDamage = 5\r
  SecondaryDamageRadius = 4\r
  AttackRange = 40\r
  DelayBetweenShots = 1000\r
End\r
Weapon Gatling\r
  PrimaryDamage = 2\r
  ScatterRadiusVsInfantry = 3\r
  AttackRange = 120\r
  DelayBetweenShots = 100\r
End\r
Weapon Bolt\r
  PrimaryDamage = 30\r
  AttackRange = 100\r
  WeaponSpeed = 2400\r
  DelayBetweenShots = 1000\r
End\r
Weapon Stinger\r
  PrimaryDamage = 30\r
  AttackRange = 300\r
  AntiGround = No\r
  AntiAirborneVehicle = Yes\r
  DelayBetweenShots = 1000\r
End\r
Weapon Blast\r
  PrimaryDamage = 500\r
  AttackRange = 5\r
  RadiusDamageAffects = ALLIES ENEMIES SUICIDE\r
  DelayBetweenShots = 1000\r
End\r
Weapon Pulse\r
  PrimaryDamage = 50\r
  AttackRange = 30\r
  DamageDealtAtSelfPosition = Yes\r
  DelayBetweenShots = 1000\r
End\r
Weapon Minigun\r
  PrimaryDamage = 1\r
  AttackRange = 150\r
  DelayBetweenShots = 200\r
  ContinuousFireOne = 10\r
End\r
Weapon Disabler\r
  PrimaryDamage = 100\r
  AttackRange = 150\r
  DamageType = SUBDUAL_VEHICLE\r
  DelayBetweenShots = 100\r
End\r
Weapon Mender\r
  PrimaryDamage = 5\r
  AttackRange = 50\r
  DamageType = HEALING\r
  DelayBetweenShots = 1000\r
End\r
Weapon Beam\r
  PrimaryDamage = 30\r
  AttackRange = 100\r
  WeaponSpeed = 2400\r
  LaserName = BeamLaser\r
  DelayBetweenShots = 1000\r
End\r
";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weapon_reads_its_numbers_its_type_its_times_and_its_projectile() {
        let mut ini = WeaponIni::default();
        ini.read(internals::WEAPONS.as_bytes()).unwrap();
        let rifle = ini.get("RangerRifle").unwrap();
        assert_eq!(
            *rifle,
            IniWeapon {
                damage: 5.0,
                radius: 0.0,
                secondary_radius: 0.0,
                scatter: 0.0,
                infantry_scatter: 0.0,
                range: 100.0,
                speed: IniWeapon::INSTANT,
                laser: false,
                anti_ground: true,
                suicide: false,
                at_self: false,
                continuous_fire: false,
                min_range: 0.0,
                damage_type: Some("SMALL_ARMS".to_owned()),
                delay: ShotDelay::Fixed(100),
                clip_size: 3,
                clip_reload_ms: 700,
                pre_attack_ms: 0,
                projectile: None,
            }
        );
        assert_eq!(
            ini.get("TankCannon").unwrap().projectile.as_deref(),
            Some("TankShell")
        );
        assert_eq!(ini.get("Pistol").unwrap().delay, ShotDelay::Random);
        // A weapon that names no type is of the default, which the import gives it.
        assert_eq!(ini.get("Mortar").unwrap().damage_type, None);
        assert_eq!(ini.get("Knife").unwrap().pre_attack_ms, 833);
        assert!(ini.get("rangerrifle").is_none());
        // `SUICIDE` is held by a list of it, or added by `+`, and gone after `NONE`, a list
        // without it, or `-`.
        let affects = |lines: &str| {
            let mut ini = WeaponIni::default();
            ini.read(format!("Weapon W\n{lines}End\n").as_bytes())
                .unwrap();
            ini.get("W").unwrap().suicide
        };
        assert!(affects("  RadiusDamageAffects = ENEMIES SUICIDE\n"));
        assert!(affects("  RadiusDamageAffects = +SUICIDE\n"));
        assert!(!affects(
            "  RadiusDamageAffects = SUICIDE\n  RadiusDamageAffects = NONE\n"
        ));
        assert!(!affects(
            "  RadiusDamageAffects = SUICIDE\n  RadiusDamageAffects = ENEMIES\n"
        ));
        assert!(!affects("  RadiusDamageAffects = +SUICIDE -SUICIDE\n"));
        assert!(ini.get("Minigun").unwrap().continuous_fire);
        assert!(!ini.get("Stinger").unwrap().anti_ground);
        // A line outside a block, and a block left open, are refused.
        assert!(WeaponIni::default().read(b"PrimaryDamage = 5\r\n").is_err());
        assert!(WeaponIni::default().read(b"Weapon Open\r\n").is_err());
    }
}
