use std::collections::BTreeMap;

use crate::zero_hour::format::ini::damage_type::DamageType;
use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, FlagSet, IniLine, IniText};

/// The weapons of Zero Hour's `Weapon.ini` files, as `WeaponStore` reads them, by their names, of
/// their case.
#[derive(Debug, Default)]
pub(crate) struct WeaponIni {
    weapons: BTreeMap<String, IniWeapon>,
}

/// What the import reads of a weapon (`WeaponTemplate`), each the template's default when absent:
/// its `PrimaryDamage`, `PrimaryDamageRadius`, `SecondaryDamageRadius`, `ScatterRadius`,
/// `ScatterRadiusVsInfantry`, `AttackRange` and `MinimumAttackRange`; its `WeaponSpeed` in units
/// a second, `INSTANT` by default; its `MinTargetPitch` and `MaxTargetPitch` in degrees, −180 and
/// 180 by default; whether it names a `LaserName`; whether it may hit the ground, `AntiGround`,
/// yes by default; its `RadiusDamageAffects` and its `ProjectileCollidesWith`; whether it deals its
/// damage at its own place, `DamageDealtAtSelfPosition`; whether it fires faster as it
/// keeps firing, of a `ContinuousFireOne` or a `ContinuousFireTwo`; its `DamageType`, of the
/// game's types, `EXPLOSION` by default; its `DelayBetweenShots`, `ClipSize`,
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
    pub(crate) pitch: [f32; 2],
    pub(crate) laser: bool,
    pub(crate) anti_ground: bool,
    pub(crate) affects: RadiusAffects,
    pub(crate) collides: CollideMask,
    pub(crate) at_self: bool,
    pub(crate) continuous_fire: bool,
    pub(crate) min_range: f32,
    pub(crate) damage_type: DamageType,
    pub(crate) delay: ShotDelay,
    pub(crate) clip_size: i32,
    pub(crate) clip_reload_ms: i32,
    pub(crate) pre_attack_ms: i32,
    pub(crate) projectile: Option<String>,
}

/// A weapon's `RadiusDamageAffects`, of `TheWeaponAffectsMaskNames`: whom its radius damages, by
/// their relation to its unit, its unit itself, `SELF`, and whether it kills its own unit, spares
/// the units of its unit's own template, or spares the airborne; `ALLIES ENEMIES NEUTRALS` by
/// default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RadiusAffects {
    pub(crate) source: bool,
    pub(crate) allies: bool,
    pub(crate) enemies: bool,
    pub(crate) neutrals: bool,
    pub(crate) suicide: bool,
    pub(crate) not_similar: bool,
    pub(crate) not_airborne: bool,
}

/// A weapon's `ProjectileCollidesWith`, of `TheWeaponCollideMaskNames`: what its projectile
/// detonates on in its way, `STRUCTURES` by default (`shouldProjectileCollideWith`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CollideMask {
    pub(crate) allies: bool,
    pub(crate) enemies: bool,
    pub(crate) structures: bool,
    pub(crate) shrubbery: bool,
    pub(crate) projectiles: bool,
    pub(crate) walls: bool,
    pub(crate) small_missiles: bool,
    pub(crate) ballistic_missiles: bool,
    pub(crate) controlled_structures: bool,
}

/// A weapon's `DelayBetweenShots`: one time in milliseconds, or a range of `Min:` and `Max:`, of
/// which the game draws one for each shot (`WeaponTemplate::parseShotDelay`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShotDelay {
    Fixed(i32),
    Random,
}

impl Default for IniWeapon {
    fn default() -> Self {
        Self {
            damage: 0.0,
            radius: 0.0,
            secondary_radius: 0.0,
            scatter: 0.0,
            infantry_scatter: 0.0,
            range: 0.0,
            speed: Self::INSTANT,
            pitch: [-180.0, 180.0],
            laser: false,
            anti_ground: true,
            affects: RadiusAffects::DEFAULT,
            collides: CollideMask::DEFAULT,
            at_self: false,
            continuous_fire: false,
            min_range: 0.0,
            damage_type: DamageType::Explosion,
            delay: ShotDelay::default(),
            clip_size: 0,
            clip_reload_ms: 0,
            pre_attack_ms: 0,
            projectile: None,
        }
    }
}

impl Default for ShotDelay {
    fn default() -> Self {
        Self::Fixed(0)
    }
}

impl WeaponIni {
    /// Reads one INI file, adding its weapons to those read before: one read again keeps its
    /// fields and takes the new ones.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes).blocks(BlockKinds::One("weapon")) {
            let block = block?;
            let weapon = self.weapons.entry(block.name()?.to_owned()).or_default();
            for line in &block.fields {
                weapon.field(&line.word(), line)?;
            }
        }
        Ok(())
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
            "mintargetpitch" => self.pitch[0] = at.real()?,
            "maxtargetpitch" => self.pitch[1] = at.real()?,
            "lasername" => self.laser = at.tokens.len() > 1,
            "antiground" => self.anti_ground = at.yes()?,
            "radiusdamageaffects" => at.set_flags(&mut self.affects)?,
            "projectilecollideswith" => at.set_flags(&mut self.collides)?,
            "damagedealtatselfposition" => self.at_self = at.yes()?,
            "continuousfireone" | "continuousfiretwo" => self.continuous_fire = true,
            "minimumattackrange" => self.min_range = at.real()?,
            "damagetype" => {
                self.damage_type =
                    DamageType::of(at.value()?).ok_or(IniError::NotListed { line: at.number })?;
            }
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
}

impl RadiusAffects {
    /// `DEFAULT_AFFECTS_MASK`.
    const DEFAULT: Self = Self {
        allies: true,
        enemies: true,
        neutrals: true,
        ..Self::NONE
    };

    pub(crate) const NONE: Self = Self {
        source: false,
        allies: false,
        enemies: false,
        neutrals: false,
        suicide: false,
        not_similar: false,
        not_airborne: false,
    };
}

impl FlagSet for RadiusAffects {
    fn clear(&mut self) {
        *self = Self::NONE;
    }

    /// One of the game's names; another is refused, as `BitFlags::parse` refuses it.
    fn set(&mut self, name: &str, on: bool) -> bool {
        let flag = match name.to_ascii_lowercase().as_str() {
            "self" => &mut self.source,
            "allies" => &mut self.allies,
            "enemies" => &mut self.enemies,
            "neutrals" => &mut self.neutrals,
            "suicide" => &mut self.suicide,
            "not_similar" => &mut self.not_similar,
            "not_airborne" => &mut self.not_airborne,
            _ => return false,
        };
        *flag = on;
        true
    }
}

impl CollideMask {
    /// `DEFAULT_COLLIDE_MASK`.
    const DEFAULT: Self = Self {
        structures: true,
        ..Self::NONE
    };

    const NONE: Self = Self {
        allies: false,
        enemies: false,
        structures: false,
        shrubbery: false,
        projectiles: false,
        walls: false,
        small_missiles: false,
        ballistic_missiles: false,
        controlled_structures: false,
    };
}

impl FlagSet for CollideMask {
    fn clear(&mut self) {
        *self = Self::NONE;
    }

    /// One of the game's names; another is refused, as `BitFlags::parse` refuses it.
    fn set(&mut self, name: &str, on: bool) -> bool {
        let flag = match name.to_ascii_lowercase().as_str() {
            "allies" => &mut self.allies,
            "enemies" => &mut self.enemies,
            "structures" => &mut self.structures,
            "shrubbery" => &mut self.shrubbery,
            "projectiles" => &mut self.projectiles,
            "walls" => &mut self.walls,
            "small_missiles" => &mut self.small_missiles,
            "ballistic_missiles" => &mut self.ballistic_missiles,
            "controlled_structures" => &mut self.controlled_structures,
            _ => return false,
        };
        *flag = on;
        true
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// `Data\INI\Weapon.ini` of the fixture install: a rifle that fires a clip of 3 at once, a
    /// cannon that fires a shell of a radius, a scatter at infantry and a bound pitch, a launcher
    /// of a rocket, a phantom of a projectile no file defines, a lobber whose radius spares its own
    /// kind, a popper whose shell detonates on shrubbery, a flamer of a radius, a knife that waits
    /// before it strikes, a mortar of a minimum range, a pistol of a random delay, a marker of no
    /// delay, a shotgun of a second radius, a gatling that scatters at infantry, a bolt of a set
    /// speed, which may delay its damage, a stinger that hits no ground, a blast that kills its own
    /// unit, a pulse dealt at its own place, a minigun that fires faster as it keeps firing, a
    /// disabler of subdual damage, a mender of healing damage, and a beam of the bolt's speed, a
    /// laser, which does not delay its damage.
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
  PrimaryDamageRadius = 5\r
  ScatterRadiusVsInfantry = 10\r
  AttackRange = 150\r
  MinTargetPitch = -15\r
  MaxTargetPitch = 15\r
  DamageType = ARMOR_PIERCING\r
  WeaponSpeed = 400\r
  DelayBetweenShots = 2000\r
  ProjectileObject = TankShell\r
  RadiusDamageAffects = ALLIES ENEMIES NEUTRALS\r
  ProjectileCollidesWith = STRUCTURES WALLS\r
End\r
Weapon Launcher\r
  PrimaryDamage = 40\r
  AttackRange = 200\r
  DelayBetweenShots = 1000\r
  ProjectileObject = Rocket\r
End\r
Weapon Phantom\r
  PrimaryDamage = 40\r
  AttackRange = 200\r
  DelayBetweenShots = 1000\r
  ProjectileObject = NoSuchShell\r
End\r
Weapon Lobber\r
  PrimaryDamage = 20\r
  PrimaryDamageRadius = 10\r
  AttackRange = 200\r
  DelayBetweenShots = 1000\r
  ProjectileObject = TankShell\r
  RadiusDamageAffects = ENEMIES NOT_SIMILAR\r
End\r
Weapon Popper\r
  PrimaryDamage = 20\r
  AttackRange = 200\r
  DelayBetweenShots = 1000\r
  ProjectileObject = TankShell\r
  ProjectileCollidesWith = STRUCTURES SHRUBBERY\r
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
        ini.read(samples::WEAPONS.as_bytes()).unwrap();
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
                pitch: [-180.0, 180.0],
                laser: false,
                anti_ground: true,
                affects: RadiusAffects::DEFAULT,
                collides: CollideMask::DEFAULT,
                at_self: false,
                continuous_fire: false,
                min_range: 0.0,
                damage_type: DamageType::SmallArms,
                delay: ShotDelay::Fixed(100),
                clip_size: 3,
                clip_reload_ms: 700,
                pre_attack_ms: 0,
                projectile: None,
            }
        );
        // The cannon's shell detonates on structures and walls, and its radius damages every
        // relation but its own unit; its pitch is read in degrees.
        let cannon = ini.get("TankCannon").unwrap();
        assert_eq!(cannon.projectile.as_deref(), Some("TankShell"));
        assert_eq!(cannon.pitch, [-15.0, 15.0]);
        let collides = CollideMask {
            walls: true,
            ..CollideMask::DEFAULT
        };
        assert_eq!(cannon.collides, collides);
        assert_eq!(cannon.affects, RadiusAffects::DEFAULT);
        assert_eq!(ini.get("Pistol").unwrap().delay, ShotDelay::Random);
        // A weapon that names no type is of the default; a type of none of the game's names is
        // refused, as the game's reader refuses it.
        assert_eq!(
            ini.get("Mortar").unwrap().damage_type,
            DamageType::Explosion
        );
        let mut other = WeaponIni::default();
        assert_eq!(
            other.read(b"Weapon W\n  DamageType = SPLASH\nEnd\n"),
            Err(IniError::NotListed { line: 2 })
        );
        assert_eq!(ini.get("Knife").unwrap().pre_attack_ms, 833);
        assert!(ini.get("rangerrifle").is_none());
        // `SUICIDE` is held by a list of it, or added by `+`, and gone after `NONE`, a list
        // without it, or `-`.
        let affects = |lines: &str| {
            let mut ini = WeaponIni::default();
            ini.read(format!("Weapon W\n{lines}End\n").as_bytes())
                .unwrap();
            ini.get("W").unwrap().affects.suicide
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
        // `SELF` is the unit itself; a name the game does not list is refused, as its reader
        // refuses it.
        let mut own = WeaponIni::default();
        own.read(b"Weapon W\n  RadiusDamageAffects = SELF NOT_SIMILAR\nEnd\n")
            .unwrap();
        let own = own.get("W").unwrap().affects;
        let expected = RadiusAffects {
            source: true,
            not_similar: true,
            ..RadiusAffects::NONE
        };
        assert_eq!(own, expected);
        for line in [
            "RadiusDamageAffects = FRIENDS",
            "ProjectileCollidesWith = STRUCTURES TREES",
        ] {
            let text = format!("Weapon W\n  {line}\nEnd\n");
            let read = WeaponIni::default().read(text.as_bytes());
            assert_eq!(read, Err(IniError::NotListed { line: 2 }), "{line}");
        }
        assert!(ini.get("Minigun").unwrap().continuous_fire);
        assert!(!ini.get("Stinger").unwrap().anti_ground);
        // A line outside a block, and a block left open, are refused.
        assert!(WeaponIni::default().read(b"PrimaryDamage = 5\r\n").is_err());
        assert!(WeaponIni::default().read(b"Weapon Open\r\n").is_err());
    }
}
