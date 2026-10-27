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
/// `ScatterRadiusVsInfantry`, `AttackRange` and `MinimumAttackRange`; its `WeaponSpeed` in units a
/// second, `INSTANT` by default; its `MinTargetPitch` and `MaxTargetPitch` in degrees, −180 and 180
/// by default; whether it names a `LaserName`; whether it may hit the ground, `AntiGround`, yes by
/// default, and a vehicle in the air, `AntiAirborneVehicle`, no by default, as the template starts
/// its anti mask (`WEAPON_ANTI_GROUND`); its `RadiusDamageAffects` and its
/// `ProjectileCollidesWith`; whether it deals its damage at its own place,
/// `DamageDealtAtSelfPosition`; its continuous fire; its bonuses of `PLAYER_UPGRADE`, 1 by default,
/// as `WeaponBonus` starts; its `DamageType`, of the game's types, `EXPLOSION` by default; its
/// `DelayBetweenShots`, `ClipSize`, `ClipReloadTime` and `PreAttackDelay`, times in milliseconds;
/// its `PreAttackType`, `PER_SHOT` by default; and its `ProjectileObject`, none for `NONE`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IniWeapon {
    pub(crate) damage: f32,
    pub(crate) radius: f32,
    pub(crate) secondary_damage: f32,
    pub(crate) secondary_radius: f32,
    pub(crate) scatter: f32,
    pub(crate) infantry_scatter: f32,
    pub(crate) range: f32,
    pub(crate) speed: f32,
    pub(crate) pitch: [f32; 2],
    /// Its `AcceptableAimDelta` in degrees, 0 by default.
    pub(crate) aim_delta: f32,
    pub(crate) laser: bool,
    pub(crate) anti_ground: bool,
    pub(crate) anti_air: bool,
    pub(crate) affects: RadiusAffects,
    pub(crate) collides: CollideMask,
    pub(crate) at_self: bool,
    pub(crate) continuous_fire: ContinuousFire,
    pub(crate) upgrade: UpgradeBonus,
    pub(crate) min_range: f32,
    pub(crate) damage_type: DamageType,
    pub(crate) delay: ShotDelay,
    pub(crate) clip_size: i32,
    pub(crate) clip_reload_ms: i32,
    /// Whether its `AutoReloadsClip` is `RETURN_TO_BASE`: its clip reloads at its unit's home
    /// alone.
    pub(crate) reloads_at_home: bool,
    pub(crate) pre_attack_ms: i32,
    pub(crate) pre_attack_type: PrefireType,
    pub(crate) projectile: Option<String>,
    /// It names a `ProjectileStreamName`, which draws its projectiles as a stream.
    pub(crate) stream: bool,
    /// It names a `ProjectileDetonationOCL`, the objects its projectile creates where it
    /// detonates.
    pub(crate) detonation_objects: bool,
    /// The effect list it plays as it fires, its `FireFX`; the one where its projectile
    /// detonates or its shot of no projectile strikes, its `ProjectileDetonationFX`; and the
    /// particle system its projectile carries, its `ProjectileExhaust`; each none for `NONE`.
    pub(crate) fire_fx: Option<String>,
    pub(crate) detonation_fx: Option<String>,
    pub(crate) exhaust: Option<String>,
    /// The audio event it plays as it fires, its `FireSound`, none for `NONE`; and its
    /// `FireSoundLoopTime`, in milliseconds, over which a weapon that fires again loops it, 0 for
    /// none.
    pub(crate) fire_sound: Option<String>,
    pub(crate) fire_sound_loop_ms: i32,
}

/// How a weapon fires faster as it keeps firing, as `FiringTracker` reads its template: the shots
/// in a row past which its object reaches `CONTINUOUS_FIRE_MEAN` and `CONTINUOUS_FIRE_FAST`,
/// `ContinuousFireOne` and `ContinuousFireTwo`, 0 by default; the milliseconds its object keeps a
/// stage with no shot, `ContinuousFireCoast`; and each stage's bonus to each field of
/// `BONUS_FIELDS`, of its `WeaponBonus` lines, 1 by default, as `WeaponBonus` starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ContinuousFire {
    pub(crate) shots: [i32; 2],
    pub(crate) coast_ms: i32,
    bonuses: [[f32; BONUS_FIELDS.len()]; 2],
}

/// What a weapon's object gains with `PLAYER_UPGRADE`, as a `WeaponBonusUpgrade` sets it: its
/// bonus to each field of `BONUS_FIELDS`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct UpgradeBonus {
    bonuses: [f32; BONUS_FIELDS.len()],
}

/// `TheWeaponBonusNames`, of the game's build, which leaves `ALLOW_DEMORALIZE` out: a bonus's
/// condition, the two continuous-fire stages at 2 and 3.
const BONUS_CONDITIONS: [&str; 27] = [
    "GARRISONED",
    "HORDE",
    "CONTINUOUS_FIRE_MEAN",
    "CONTINUOUS_FIRE_FAST",
    "NATIONALISM",
    "PLAYER_UPGRADE",
    "DRONE_SPOTTING",
    "DEMORALIZED_OBSOLETE",
    "ENTHUSIASTIC",
    "VETERAN",
    "ELITE",
    "HERO",
    "BATTLEPLAN_BOMBARDMENT",
    "BATTLEPLAN_HOLDTHELINE",
    "BATTLEPLAN_SEARCHANDDESTROY",
    "SUBLIMINAL",
    "SOLO_HUMAN_EASY",
    "SOLO_HUMAN_NORMAL",
    "SOLO_HUMAN_HARD",
    "SOLO_AI_EASY",
    "SOLO_AI_NORMAL",
    "SOLO_AI_HARD",
    "TARGET_FAERIE_FIRE",
    "FANATICISM",
    "FRENZY_ONE",
    "FRENZY_TWO",
    "FRENZY_THREE",
];

/// The place of `CONTINUOUS_FIRE_MEAN` among `BONUS_CONDITIONS`, `FAST` the next.
const CONTINUOUS_FIRE_MEAN: usize = 2;

/// The place of `PLAYER_UPGRADE` among `BONUS_CONDITIONS`.
const PLAYER_UPGRADE: usize = 5;

/// `TheWeaponBonusFieldNames`: what a bonus changes, the rate of fire at 3.
const BONUS_FIELDS: [&str; 5] = ["DAMAGE", "RADIUS", "RANGE", "RATE_OF_FIRE", "PRE_ATTACK"];

/// The place of `RATE_OF_FIRE` among `BONUS_FIELDS`.
const RATE_OF_FIRE: usize = 3;

/// The place of `DAMAGE` among `BONUS_FIELDS`.
const DAMAGE: usize = 0;

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

/// What starts a weapon's `PreAttackDelay` again, its `PreAttackType` (`WeaponPrefireType`): each
/// shot, each target, or each clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrefireType {
    Shot,
    Attack,
    Clip,
}

impl PrefireType {
    /// The type `name` names, of `TheWeaponPrefireNames`, in any case.
    fn of(name: &str) -> Option<Self> {
        [
            ("PER_SHOT", Self::Shot),
            ("PER_ATTACK", Self::Attack),
            ("PER_CLIP", Self::Clip),
        ]
        .into_iter()
        .find_map(|(named, kind)| name.eq_ignore_ascii_case(named).then_some(kind))
    }
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
            secondary_damage: 0.0,
            secondary_radius: 0.0,
            scatter: 0.0,
            infantry_scatter: 0.0,
            range: 0.0,
            speed: Self::INSTANT,
            pitch: [-180.0, 180.0],
            aim_delta: 0.0,
            laser: false,
            anti_ground: true,
            anti_air: false,
            affects: RadiusAffects::DEFAULT,
            collides: CollideMask::DEFAULT,
            at_self: false,
            continuous_fire: ContinuousFire::default(),
            upgrade: UpgradeBonus::default(),
            min_range: 0.0,
            damage_type: DamageType::Explosion,
            delay: ShotDelay::default(),
            clip_size: 0,
            clip_reload_ms: 0,
            reloads_at_home: false,
            pre_attack_ms: 0,
            pre_attack_type: PrefireType::Shot,
            projectile: None,
            stream: false,
            detonation_objects: false,
            fire_fx: None,
            detonation_fx: None,
            fire_sound: None,
            fire_sound_loop_ms: 0,
            exhaust: None,
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
        for block in IniText::of(bytes).lines().blocks(BlockKinds::One("weapon")) {
            let block = block?;
            let weapon = self.weapons.entry(block.name()?.to_owned()).or_default();
            for line in &block.fields {
                weapon.field(line.word(), line)?;
            }
        }
        Ok(())
    }

    /// The weapon `name`; none for a name no file defines.
    pub(crate) fn get(&self, name: &str) -> Option<&IniWeapon> {
        self.weapons.get(name)
    }

    /// Its weapons, by their names.
    pub(crate) const fn weapons(&self) -> &BTreeMap<String, IniWeapon> {
        &self.weapons
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
            "secondarydamage" => self.secondary_damage = at.real()?,
            "secondarydamageradius" => self.secondary_radius = at.real()?,
            "scatterradius" => self.scatter = at.real()?,
            "scatterradiusvsinfantry" => self.infantry_scatter = at.real()?,
            "attackrange" => self.range = at.real()?,
            "weaponspeed" => self.speed = at.real()?,
            "mintargetpitch" => self.pitch[0] = at.real()?,
            "maxtargetpitch" => self.pitch[1] = at.real()?,
            "acceptableaimdelta" => self.aim_delta = at.real()?,
            "lasername" => self.laser = at.tokens.len() > 1,
            "antiground" => self.anti_ground = at.yes()?,
            "antiairbornevehicle" => self.anti_air = at.yes()?,
            "radiusdamageaffects" => at.set_flags(&mut self.affects)?,
            "projectilecollideswith" => at.set_flags(&mut self.collides)?,
            "damagedealtatselfposition" => self.at_self = at.yes()?,
            "continuousfireone" => self.continuous_fire.shots[0] = at.int()?,
            "continuousfiretwo" => self.continuous_fire.shots[1] = at.int()?,
            "continuousfirecoast" => self.continuous_fire.coast_ms = at.int()?,
            "weaponbonus" => self.bonus(at)?,
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
            "autoreloadsclip" => {
                self.reloads_at_home = at.value()?.eq_ignore_ascii_case("return_to_base");
            }
            "preattackdelay" => self.pre_attack_ms = at.int()?,
            "preattacktype" => {
                self.pre_attack_type =
                    PrefireType::of(at.value()?).ok_or(IniError::NotListed { line: at.number })?;
            }
            "projectileobject" => self.projectile = none(at.value()?),
            "projectilestreamname" => self.stream = none(at.value()?).is_some(),
            "projectiledetonationocl" => self.detonation_objects = none(at.value()?).is_some(),
            "firefx" => self.fire_fx = none(at.value()?),
            "projectiledetonationfx" => self.detonation_fx = none(at.value()?),
            "projectileexhaust" => self.exhaust = none(at.value()?),
            "firesound" => self.fire_sound = none(at.value()?),
            "firesoundlooptime" => self.fire_sound_loop_ms = at.int()?,
            _ => {}
        }
        Ok(())
    }
}

impl Default for ContinuousFire {
    fn default() -> Self {
        Self {
            shots: [0; 2],
            coast_ms: 0,
            bonuses: [[1.0; BONUS_FIELDS.len()]; 2],
        }
    }
}

impl Default for UpgradeBonus {
    fn default() -> Self {
        Self {
            bonuses: [1.0; BONUS_FIELDS.len()],
        }
    }
}

impl UpgradeBonus {
    /// Its bonus to the damage.
    pub(crate) const fn damage(&self) -> f32 {
        self.bonuses[DAMAGE]
    }

    /// Whether it changes a field other than the damage, its bonus to it not 1.
    pub(crate) fn changes_other_fields(&self) -> bool {
        let mut fields = self.bonuses.iter().enumerate();
        fields.any(|(at, &bonus)| at != DAMAGE && bonus != 1.0)
    }
}

impl ContinuousFire {
    /// The rate of fire each stage gives, `MEAN`'s and `FAST`'s.
    pub(crate) const fn rates(&self) -> [f32; 2] {
        [self.bonuses[0][RATE_OF_FIRE], self.bonuses[1][RATE_OF_FIRE]]
    }

    /// Whether a stage changes a field other than the rate of fire, its bonus to it not 1.
    pub(crate) fn changes_other_fields(&self) -> bool {
        self.bonuses.iter().any(|fields| {
            let mut fields = fields.iter().enumerate();
            fields.any(|(at, &bonus)| at != RATE_OF_FIRE && bonus != 1.0)
        })
    }
}

impl IniWeapon {
    /// Takes a `WeaponBonus` line, its condition, its field and its percent, as
    /// `WeaponBonusSet::parseWeaponBonusSet` sets one field of one condition, a later line of the
    /// same over the earlier, keeping the continuous-fire stages' and `PLAYER_UPGRADE`'s; a name
    /// of no list fails the read, as `INI::scanIndexList` does.
    fn bonus(&mut self, at: &IniLine<'_>) -> Result<(), IniError> {
        let listed = |token: Option<&&str>, names: &[&str]| {
            let token = token.ok_or(IniError::NoValue { line: at.number })?;
            let found = names
                .iter()
                .position(|name| name.eq_ignore_ascii_case(token));
            found.ok_or(IniError::NotListed { line: at.number })
        };
        let condition = listed(at.tokens.get(1), &BONUS_CONDITIONS)?;
        let field = listed(at.tokens.get(2), &BONUS_FIELDS)?;
        // `INI::scanPercentToReal` divides the real by 100 in the game's `f32`.
        let value = at.real_at(3)? / 100.0;
        if condition == PLAYER_UPGRADE {
            self.upgrade.bonuses[field] = value;
            return Ok(());
        }
        let Some(stage) = condition
            .checked_sub(CONTINUOUS_FIRE_MEAN)
            .filter(|&stage| stage < 2)
        else {
            return Ok(());
        };
        self.continuous_fire.bonuses[stage][field] = value;
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
    /// cannon that fires a shell of a radius, a scatter at infantry, a bound pitch and an aim of
    /// 30°, a launcher
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
  AcceptableAimDelta = 30\r
  DamageType = ARMOR_PIERCING\r
  WeaponSpeed = 400\r
  DelayBetweenShots = 2000\r
  ProjectileObject = TankShell\r
  FireFX = FX_TankFire\r
  FireSound = TankCannonFire\r
  FireSoundLoopTime = 80\r
  ProjectileDetonationFX = FX_TankHit\r
  ProjectileExhaust = NONE\r
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
  PreAttackType = PER_ATTACK\r
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
  WeaponBonus = PLAYER_UPGRADE DAMAGE 125%\r
End\r
Weapon Stinger\r
  PrimaryDamage = 30\r
  AttackRange = 300\r
  AntiGround = No\r
  AntiAirborneVehicle = Yes\r
  DelayBetweenShots = 1000\r
End\r
Weapon JetMissile\r
  PrimaryDamage = 100\r
  AttackRange = 320\r
  DelayBetweenShots = 150\r
  ClipSize = 4\r
  ClipReloadTime = 8000\r
  AutoReloadsClip = RETURN_TO_BASE\r
End\r
Weapon Interceptor\r
  PrimaryDamage = 10\r
  AttackRange = 200\r
  AntiGround = No\r
  AntiProjectile = Yes\r
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
  DamageType = GATTLING\r
  DelayBetweenShots = 250\r
  ContinuousFireOne = 1\r
  ContinuousFireTwo = 5\r
  ContinuousFireCoast = 2000\r
  WeaponBonus = CONTINUOUS_FIRE_MEAN RATE_OF_FIRE 200%\r
  WeaponBonus = continuous_fire_fast rate_of_fire 300%\r
  WeaponBonus = PLAYER_UPGRADE DAMAGE 125%\r
End\r
Weapon Spinner\r
  PrimaryDamage = 1\r
  AttackRange = 150\r
  DelayBetweenShots = 250\r
  ContinuousFireOne = 2\r
  ContinuousFireTwo = 4\r
  ContinuousFireCoast = 1000\r
  WeaponBonus = CONTINUOUS_FIRE_MEAN RATE_OF_FIRE 150%\r
  WeaponBonus = CONTINUOUS_FIRE_FAST RATE_OF_FIRE 200%\r
  WeaponBonus = CONTINUOUS_FIRE_FAST DAMAGE 150%\r
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
mod tests;
