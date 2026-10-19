use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use std::num::NonZeroU8;

use campfire_actions::{ActionKind, ActionRange, DeliveryData, RangeField, Targeting};
use campfire_client_files::ClientUnits;
use campfire_combat::{AttackData, ClipData, PitchData, ScatterData, SplashData, WeaponValue};
use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_projectiles::ProjectileData;
use campfire_values::{FilterData, Number, Ranked, RelationSet, Scalar, Speed};
use serde::Serialize;

use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::damage_type::DamageType;
use crate::zero_hour::format::ini::object_ini::{IniObject, ObjectIni};
use crate::zero_hour::format::ini::weapon_ini::{IniWeapon, RadiusAffects, ShotDelay};
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ShellNames;
use crate::zero_hour::imported_type::{INFANTRY, ImportedType};
use crate::zero_hour::weapon_drop::{self, WeaponDrop};
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// Zero Hour's combat (04-capabilities/combat.md, Armor by damage kind; 12-zero-hour.md, Life is
/// the original's; 12-zero-hour.md, A weapon converts when it hits one target at once;
/// 12-zero-hour.md, The import converts shells, splash, scatter, pitch, turrets and
/// slow weapons): the weapons of each unit type's starting weapon set that convert, as attack
/// actions, once for the whole import, each in its type's slot kind `weapon`; every armor, the
/// share of each damage kind it does not take whole; the weapons dropped, each once for each
/// cause, in the order the types are read; the objects whose body the import gives no life, and
/// those whose body starts below its most life; each type's actions of the slot kind `weapon`, by
/// its name, which it returns; and the shell types of a weapon's own, each with the object whose
/// look it takes.
#[derive(Debug, Default)]
pub(crate) struct CombatImport {
    pub(crate) actions: BTreeMap<DeclaredName, ImportedWeapon>,
    pub(crate) weapons: BTreeMap<ImportName, Vec<DeclaredName>>,
    pub(crate) armors: BTreeMap<DeclaredName, BTreeMap<DeclaredName, Num>>,
    pub(crate) dropped: Vec<DroppedWeapon>,
    pub(crate) unliving: Vec<String>,
    pub(crate) wounded: Vec<String>,
    shell_looks: Vec<ShellLook>,
}

/// A shell type named for its weapon, and the object whose look it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ShellLook {
    unit_type: DeclaredName,
    object: DeclaredName,
}

/// An attack as the mode's data writes it, `[actions.weapon_<name>]` (12-zero-hour.md, A weapon
/// converts when it hits one target at once): its numbers, its clip, the shell it fires, the
/// damage of its radius, its scatter, the pitches it reaches, and the speed of a weapon of no
/// shell whose damage lands after its shot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ImportedWeapon {
    kind: ActionKind,
    targeting: Targeting,
    range: Ranked<RangeField>,
    #[serde(flatten)]
    pub(crate) attack: AttackData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) delivery: Option<DeliveryData>,
}

/// A weapon the import converts, and the shell it fires, if it fires one.
#[derive(Debug, Clone)]
struct ConvertedWeapon<'a> {
    weapon: ImportedWeapon,
    shell: Option<FiredShell<'a>>,
}

/// The object a weapon fires, and its projectile as that weapon fires it.
#[derive(Debug, Clone)]
struct FiredShell<'a> {
    object: &'a IniObject,
    projectile: ProjectileData,
}

/// A weapon the import drops, by its name, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedWeapon {
    pub weapon: String,
    pub cause: WeaponDrop,
}

/// A quarter turn, the steepest pitch, in degrees.
const QUARTER_DEG: f32 = 90.0;

impl CombatImport {
    /// The combat of `rules`, of the unit types `names` made, each type's weapons by its name;
    /// the shell types its weapons fire join `names`. An error for a type whose armor no file
    /// defines by the name of its case, which the game's debug build stops at as wrong data and
    /// its release build takes as no armor (`INI::parseArmorTemplate`), and for a shell type's
    /// name another object took.
    pub(crate) fn of(rules: &ZeroHourRules, names: &mut ShellNames) -> Result<Self, ZeroHourError> {
        let mut import = Self::default();
        let mut armors = BTreeMap::new();
        for (name, armor) in rules.armors.iter() {
            let id = ImportName::latin(name).declared();
            let mut shares = BTreeMap::new();
            for kind in DamageType::ALL {
                let share = armor.share(kind);
                if share == 1.0 {
                    continue;
                }
                let share = game_float::to_num(share)
                    .ok()
                    .filter(|share| *share >= Num::ZERO)
                    .ok_or_else(|| ZeroHourError::ArmorShare(name.to_owned()))?;
                shares.insert(kind.kind(), share);
            }
            if armors.insert(id, shares).is_some() {
                return Err(ZeroHourError::ArmorName(name.to_owned()));
            }
        }
        // Each converted weapon's shell, by its action and with its weapon's name.
        let mut shells = BTreeMap::<DeclaredName, (&str, FiredShell<'_>)>::new();
        for (type_name, object_name, typed) in names.types() {
            let object = rules
                .objects
                .object(object_name.as_bytes())
                .expect("a typed object is in the file");
            let fight = object.fight();
            if typed.armor().is_some() {
                let armor = fight
                    .starting_armor()
                    .expect("a type's armor is its object's");
                if rules.armors.get(armor).is_none() {
                    return Err(ZeroHourError::NoArmor {
                        object: object.name.clone(),
                        armor: armor.to_owned(),
                    });
                }
            }
            match (fight.body, typed.life()) {
                (Some(body), Some(_)) if body.initial < body.max => {
                    import.wounded.push(object.name.clone());
                }
                (Some(_), None) => import.unliving.push(object.name.clone()),
                _ => {}
            }
            let Some(set) = fight.starting_weapons() else {
                continue;
            };
            for weapon in set.weapons.iter().flatten() {
                let converted = rules
                    .weapons
                    .get(weapon)
                    .ok_or(WeaponDrop::NoWeapon)
                    .and_then(|ini| ImportedWeapon::of(ini, &rules.objects));
                match converted {
                    Ok(converted) => {
                        let id = DeclaredName::new(&format!(
                            "weapon_{}",
                            ImportName::latin(weapon).as_str()
                        ))
                        .expect("a kind and an import name make a declared name");
                        // Weapons whose names differ only in case would share one action.
                        match import.actions.entry(id.clone()) {
                            Entry::Occupied(held)
                                if *held.get() != converted.weapon
                                    || !FiredShell::alike(
                                        shells.get(&id).map(|(_, shell)| shell),
                                        converted.shell.as_ref(),
                                    ) =>
                            {
                                return Err(ZeroHourError::WeaponName(weapon.clone()));
                            }
                            Entry::Occupied(_) => {}
                            Entry::Vacant(new) => {
                                new.insert(converted.weapon);
                                if let Some(shell) = converted.shell {
                                    shells.insert(id.clone(), (weapon.as_str(), shell));
                                }
                            }
                        }
                        let weapons = import.weapons.entry(type_name.clone()).or_default();
                        if !weapons.contains(&id) {
                            weapons.push(id);
                        }
                    }
                    Err(cause) => {
                        let dropped = DroppedWeapon {
                            weapon: weapon.clone(),
                            cause,
                        };
                        if !import.dropped.contains(&dropped) {
                            import.dropped.push(dropped);
                        }
                    }
                }
            }
        }
        import.armors = armors;
        import.name_shells(shells, names)?;
        Ok(import)
    }

    /// Names the shell type of each of `shells`, by its action, in `names`, and delivers it from
    /// its action: an object's own name when every weapon that fires it fires it alike, else for
    /// each weapon one of the object's name and the weapon's, which takes the object's look.
    fn name_shells(
        &mut self,
        shells: BTreeMap<DeclaredName, (&str, FiredShell<'_>)>,
        names: &mut ShellNames,
    ) -> Result<(), ZeroHourError> {
        let mut by_object = BTreeMap::<&str, Vec<(DeclaredName, &str, FiredShell<'_>)>>::new();
        for (action, (weapon, shell)) in shells {
            by_object
                .entry(shell.object.name.as_str())
                .or_default()
                .push((action, weapon, shell));
        }
        for fired in by_object.into_values() {
            let alike = fired
                .windows(2)
                .all(|pair| pair[0].2.projectile == pair[1].2.projectile);
            let mut own_type: Option<DeclaredName> = None;
            for (action, weapon, FiredShell { object, projectile }) in fired {
                let own = ImportName::object(object);
                let unit_type = if let Some(named) = &own_type {
                    named.clone()
                } else if alike {
                    let named = names.shell(own, object, ImportedType::shell(projectile))?;
                    own_type.insert(named).clone()
                } else {
                    let name = ImportName::latin(&format!("{}_{weapon}", object.name));
                    let named = names.shell(name, object, ImportedType::shell(projectile))?;
                    self.shell_looks.push(ShellLook {
                        unit_type: named.clone(),
                        object: own.declared(),
                    });
                    named
                };
                let held = self
                    .actions
                    .get_mut(&action)
                    .expect("a shell's action is converted");
                held.delivery = Some(DeliveryData::Projectile {
                    unit_type,
                    count: NonZeroU8::MIN,
                    spread_deg: Num::ZERO,
                });
            }
        }
        Ok(())
    }

    /// Gives each shell type named for its weapon, in `looks`, the look of the object it fires,
    /// when that object has one.
    pub(crate) fn add_shell_looks(&self, looks: &mut ClientUnits) {
        for ShellLook { unit_type, object } in &self.shell_looks {
            if let Some(look) = looks.units.get(object) {
                let look = look.clone();
                looks.units.insert(unit_type.clone(), look);
            }
        }
    }
}

impl FiredShell<'_> {
    /// Whether `a` and `b` are one object's shell of one projectile, or both none.
    fn alike(a: Option<&Self>, b: Option<&Self>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => a.object.name == b.object.name && a.projectile == b.projectile,
            (a, b) => a.is_none() && b.is_none(),
        }
    }
}

impl ImportedWeapon {
    /// The attack of `weapon`, of the objects of `ini`, and the shell it fires; why the import
    /// drops it, when it hits no target by the engine's rules, or holds a number the engine does
    /// not.
    fn of<'a>(weapon: &IniWeapon, ini: &'a ObjectIni) -> Result<ConvertedWeapon<'a>, WeaponDrop> {
        let shell = match &weapon.projectile {
            Some(name) => Some(
                ini.object(name.as_bytes())
                    .filter(|object| object.fight().shell.is_some())
                    .ok_or(WeaponDrop::Missile)?,
            ),
            None => None,
        };
        if weapon.secondary_radius > 0.0 || (weapon.radius > 0.0 && shell.is_none()) {
            return Err(WeaponDrop::Radius);
        }
        let scatters = weapon.scatter > 0.0 || weapon.infantry_scatter > 0.0;
        if scatters && shell.is_none() {
            return Err(WeaponDrop::Scatter);
        }
        let affects = (weapon.radius > 0.0)
            .then(|| Self::relations(weapon.affects).ok_or(WeaponDrop::Affects))
            .transpose()?;
        if !weapon.anti_ground {
            return Err(WeaponDrop::NotGround);
        }
        if weapon.affects.suicide {
            return Err(WeaponDrop::Suicide);
        }
        if weapon.at_self {
            return Err(WeaponDrop::AtSelf);
        }
        if weapon.continuous_fire {
            return Err(WeaponDrop::ContinuousFire);
        }
        if weapon.pre_attack_ms > 0 {
            return Err(WeaponDrop::PreAttack);
        }
        if weapon.min_range > 0.0 {
            return Err(WeaponDrop::MinRange);
        }
        let ShotDelay::Fixed(delay) = weapon.delay else {
            return Err(WeaponDrop::RandomDelay);
        };
        let period_ms = match weapon_drop::game_ms(delay)? {
            0 => return Err(WeaponDrop::NoDelay),
            ms => ms,
        };
        let damage_kind = weapon.damage_type;
        if !damage_kind.takes_life() {
            return Err(WeaponDrop::NoLife);
        }
        if damage_kind == DamageType::Healing {
            return Err(WeaponDrop::Healing);
        }
        let number = |value: f32| {
            let value = game_float::to_num(value).map_err(|_| WeaponDrop::BadNumber)?;
            if value < Num::ZERO {
                return Err(WeaponDrop::BadNumber);
            }
            Ok(value)
        };
        let ms = |ms: u64| {
            i64::try_from(ms)
                .map(Self::own)
                .map_err(|_| WeaponDrop::BadNumber)
        };
        let clip = match weapon.clip_size {
            ..0 => return Err(WeaponDrop::BadNumber),
            0 => None,
            size => Some(ClipData {
                shots: Self::own(i64::from(size)),
                reload_ms: ms(weapon_drop::game_ms(weapon.clip_reload_ms)?)?,
            }),
        };
        let splash = match affects {
            Some(affects) => {
                let radius = number(weapon.radius)?;
                let splash = SplashData::new(radius, affects, !weapon.affects.source);
                Some(splash.ok_or(WeaponDrop::BadNumber)?)
            }
            None => None,
        };
        let scatter = if scatters {
            let mut tags = BTreeMap::new();
            if weapon.infantry_scatter > 0.0 {
                let infantry = DeclaredName::new(INFANTRY).expect("a tag is a declared name");
                tags.insert(infantry, number(weapon.infantry_scatter)?);
            }
            let scatter = ScatterData::new(number(weapon.scatter)?, tags);
            Some(scatter.ok_or(WeaponDrop::BadNumber)?)
        } else {
            None
        };
        // The original takes a bound of its defaults, a turn either way, as none.
        let [low, high] = weapon.pitch;
        if !(low.is_finite() && high.is_finite()) {
            return Err(WeaponDrop::BadNumber);
        }
        let pitch_deg = if low > -180.0 || high < 180.0 {
            let [low, high] = [low.max(-QUARTER_DEG), high.min(QUARTER_DEG)];
            let pitch = |value: f32| game_float::to_num(value).map_err(|_| WeaponDrop::BadNumber);
            Some(PitchData::new(pitch(low)?, pitch(high)?).ok_or(WeaponDrop::BadNumber)?)
        } else {
            None
        };
        // A laser's damage lands at once, and a shell's when it lands.
        let speed = if shell.is_none() && !weapon.laser && weapon.speed < IniWeapon::INSTANT {
            Some(Speed::new(number(weapon.speed)?).ok_or(WeaponDrop::BadNumber)?)
        } else {
            None
        };
        let shell = match shell {
            Some(object) => Some(FiredShell {
                object,
                projectile: ImportedType::projectile(weapon, object)?,
            }),
            None => None,
        };
        let damage = Ranked::One(Number::Value(Scalar::Decimal(number(weapon.damage)?)));
        let range = RangeField::Range(ActionRange::meters(number(weapon.range)?));
        let enemies = FilterData::parse("enemies").expect("the enemies' filter parses");
        Ok(ConvertedWeapon {
            weapon: Self {
                kind: ActionKind::Attack,
                targeting: Targeting::Unit(enemies),
                range: Ranked::One(range),
                attack: AttackData {
                    rate: WeaponValue::Own(ms(period_ms)?),
                    damage: WeaponValue::Own(damage),
                    damage_kind: damage_kind.kind(),
                    clip,
                    splash,
                    scatter,
                    pitch_deg,
                    speed,
                },
                delivery: None,
            },
            shell,
        })
    }

    /// `value` at every rank.
    fn own(value: i64) -> Ranked<Number> {
        Ranked::One(Number::from(value))
    }

    /// The relations of `affects`, as a splash's filter names them, each player a team of its
    /// own: of `ALLIES`, `ENEMIES` and `NEUTRALS` all, enemies and neutrals, or one alone; none
    /// for another set, and for a radius that spares the units of its unit's template or the
    /// airborne.
    const fn relations(affects: RadiusAffects) -> Option<RelationSet> {
        if affects.not_similar || affects.not_airborne {
            return None;
        }
        match (affects.allies, affects.enemies, affects.neutrals) {
            (true, true, true) => Some(RelationSet::All),
            (false, true, true) => Some(RelationSet::Enemies),
            (false, true, false) => Some(RelationSet::Hostiles),
            (false, false, true) => Some(RelationSet::Neutrals),
            (true, false, false) => Some(RelationSet::Allies),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_client_files::ClientUnit;

    use super::*;
    use crate::zero_hour::error::NameClash;
    use crate::zero_hour::format::ini::armor_ini::internals::ARMORS;
    use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
    use crate::zero_hour::format::ini::object_ini::internals::{DEFAULT_OBJECTS, OBJECTS};
    use crate::zero_hour::format::ini::weapon_ini::WeaponIni;
    use crate::zero_hour::format::ini::weapon_ini::internals::WEAPONS;
    use crate::zero_hour::import_names::ImportNames;
    use crate::zero_hour::imported_type::internals::shell_projectile;

    /// A rocket of no `DumbProjectileBehavior`, a missile's.
    const ROCKET: &str = "Object Rocket\n  Behavior = MissileAIUpdate ModuleTag_01\n  End\nEnd\n";

    #[test]
    fn a_weapon_converts_with_its_shell_splash_scatter_pitch_and_speed_or_drops_by_its_first_cause()
    {
        let mut ini = WeaponIni::default();
        ini.read(WEAPONS.as_bytes()).unwrap();
        let mut objects = ObjectIni::default();
        objects.read(DEFAULT_OBJECTS.as_bytes()).unwrap();
        objects.read(OBJECTS.as_bytes()).unwrap();
        objects.read(ROCKET.as_bytes()).unwrap();
        let of = |name: &str| ImportedWeapon::of(ini.get(name).unwrap(), &objects);
        let weapon = |name: &str| of(name).map(|converted| converted.weapon);
        // The rifle's 100 ms are ⌈100 × 0.03⌉ = 3 frames, written back as ⌊3 × 100 / 3⌋ = 100;
        // its reload's 700 ms are 21 frames, 700.
        let own = ImportedWeapon::own;
        let decimal = |value: i64| Ranked::One(Number::Value(Scalar::Decimal(Num::int(value))));
        let range =
            |meters: i64| Ranked::One(RangeField::Range(ActionRange::meters(Num::int(meters))));
        let rifle = ImportedWeapon {
            kind: ActionKind::Attack,
            targeting: Targeting::Unit(FilterData::parse("enemies").unwrap()),
            range: range(100),
            attack: AttackData {
                rate: WeaponValue::Own(own(100)),
                damage: WeaponValue::Own(decimal(5)),
                damage_kind: DamageType::SmallArms.kind(),
                clip: Some(ClipData {
                    shots: own(3),
                    reload_ms: own(700),
                }),
                splash: None,
                scatter: None,
                pitch_deg: None,
                speed: None,
            },
            delivery: None,
        };
        assert_eq!(weapon("RangerRifle"), Ok(rifle.clone()));
        assert!(of("RangerRifle").unwrap().shell.is_none());
        // The cannon fires its shell, which deals its damage within 5 m of where it ends to
        // every relation, its own unit spared, of no `SELF`; it scatters 10 m more at infantry,
        // and reaches from 15° below to 15° above. Its 2,000 ms are 60 frames, 2,000.
        let cannon = of("TankCannon").unwrap();
        assert_eq!(
            cannon.weapon,
            ImportedWeapon {
                range: range(150),
                attack: AttackData {
                    rate: WeaponValue::Own(own(2000)),
                    damage: WeaponValue::Own(decimal(60)),
                    damage_kind: DamageType::ArmorPiercing.kind(),
                    clip: None,
                    splash: SplashData::new(Num::int(5), RelationSet::All, true),
                    scatter: ScatterData::new(
                        Num::ZERO,
                        [(DeclaredName::new(INFANTRY).unwrap(), Num::int(10))].into()
                    ),
                    pitch_deg: PitchData::new(Num::int(-15), Num::int(15)),
                    speed: None,
                },
                ..rifle
            }
        );
        let shell = cannon.shell.unwrap();
        assert_eq!(shell.object.name, "TankShell");
        assert_eq!(shell.projectile, shell_projectile(400));
        // A projectile of no dumb behavior, a missile's, and one no file defines, are dropped.
        assert_eq!(weapon("Launcher"), Err(WeaponDrop::Missile));
        assert_eq!(weapon("Phantom"), Err(WeaponDrop::Missile));
        // A radius or a scatter of no shell, which alone ends where it is dealt or aims at its
        // scattered point; a second radius; a radius that spares its own kind; a shell that
        // detonates on shrubbery.
        assert_eq!(weapon("Flamer"), Err(WeaponDrop::Radius));
        assert_eq!(weapon("Shotgun"), Err(WeaponDrop::Radius));
        assert_eq!(weapon("Gatling"), Err(WeaponDrop::Scatter));
        assert_eq!(weapon("Lobber"), Err(WeaponDrop::Affects));
        assert_eq!(weapon("Popper"), Err(WeaponDrop::Collides));
        assert_eq!(weapon("Knife"), Err(WeaponDrop::PreAttack));
        assert_eq!(weapon("Mortar"), Err(WeaponDrop::MinRange));
        assert_eq!(weapon("Pistol"), Err(WeaponDrop::RandomDelay));
        assert_eq!(weapon("Marker"), Err(WeaponDrop::NoDelay));
        // A speed below the default lands the damage after the shot, at 2,400 m a second, but a
        // laser's lands at once.
        let bolt = weapon("Bolt").unwrap();
        assert_eq!(bolt.attack.speed.map(Speed::get), Some(Num::int(2400)));
        assert_eq!(
            weapon("Beam").map(|beam| (beam.attack.damage, beam.attack.speed)),
            Ok((WeaponValue::Own(decimal(30)), None))
        );
        assert_eq!(weapon("Stinger"), Err(WeaponDrop::NotGround));
        assert_eq!(weapon("Blast"), Err(WeaponDrop::Suicide));
        assert_eq!(weapon("Pulse"), Err(WeaponDrop::AtSelf));
        assert_eq!(weapon("Minigun"), Err(WeaponDrop::ContinuousFire));
        assert_eq!(weapon("Disabler"), Err(WeaponDrop::NoLife));
        assert_eq!(weapon("Mender"), Err(WeaponDrop::Healing));
    }

    #[test]
    fn a_pitch_bound_reads_within_a_quarter_turn_and_a_radius_its_relations() {
        let pitch = |lines: &str| {
            let mut ini = WeaponIni::default();
            let text =
                format!("Weapon W\n  PrimaryDamage = 5\n  DelayBetweenShots = 100\n{lines}End\n");
            ini.read(text.as_bytes()).unwrap();
            ImportedWeapon::of(ini.get("W").unwrap(), &ObjectIni::default())
                .map(|converted| converted.weapon.attack.pitch_deg)
        };
        // A bound of the defaults, a turn either way, is none; one side's bound keeps the
        // other's default, which the engine's quarter turn holds; a bound of no target, or of no
        // number, drops.
        assert_eq!(pitch(""), Ok(None));
        assert_eq!(
            pitch("  MaxTargetPitch = 57\n"),
            Ok(PitchData::new(Num::int(-90), Num::int(57)))
        );
        assert_eq!(
            pitch("  MinTargetPitch = 45\n  MaxTargetPitch = 80\n"),
            Ok(PitchData::new(Num::int(45), Num::int(80)))
        );
        for bound in [
            "  MinTargetPitch = 90\n",
            "  MaxTargetPitch = -90\n",
            "  MinTargetPitch = 20\n  MaxTargetPitch = 10\n",
            "  MinTargetPitch = nan\n",
        ] {
            assert_eq!(pitch(bound), Err(WeaponDrop::BadNumber), "{bound}");
        }
        // Each player a team of its own: all three relations are all, enemies and neutrals the
        // engine's enemies, and one alone its own set; two others, none, or a radius that
        // spares its own kind or the airborne, none.
        let affects = |allies, enemies, neutrals| RadiusAffects {
            allies,
            enemies,
            neutrals,
            ..RadiusAffects::NONE
        };
        let relations = [
            affects(true, true, true),
            affects(false, true, true),
            affects(false, true, false),
            affects(false, false, true),
            affects(true, false, false),
            affects(true, true, false),
            affects(true, false, true),
            affects(false, false, false),
            RadiusAffects {
                not_similar: true,
                ..affects(true, true, true)
            },
            RadiusAffects {
                not_airborne: true,
                ..affects(true, true, true)
            },
        ]
        .map(ImportedWeapon::relations);
        assert_eq!(
            relations,
            [
                Some(RelationSet::All),
                Some(RelationSet::Enemies),
                Some(RelationSet::Hostiles),
                Some(RelationSet::Neutrals),
                Some(RelationSet::Allies),
                None,
                None,
                None,
                None,
                None,
            ]
        );
    }

    /// Two soldiers, each of a gun whose name differs from the other's only in case; the second
    /// of an active body of no most life.
    const SOLDIERS: &str = "Object Gunner
  Body = ActiveBody ModuleTag_01
    MaxHealth = 10
  End
  WeaponSet
    Conditions = None
    Weapon = PRIMARY Gun
  End
End
Object Sniper
  Body = ActiveBody ModuleTag_02
  End
  WeaponSet
    Conditions = None
    Weapon = PRIMARY GUN
  End
End
";

    #[test]
    fn weapons_of_one_name_but_case_share_an_action_only_of_the_same_numbers() {
        let import = |guns: &str| {
            let mut rules = ZeroHourRules::default();
            rules.objects.read(SOLDIERS.as_bytes()).unwrap();
            rules.weapons.read(guns.as_bytes()).unwrap();
            let mut names = ImportNames::default();
            for object in ["Gunner", "Sniper"] {
                let object = rules.objects.object(object.as_bytes()).unwrap();
                names.unit_type(object, &LocomotorIni::default()).unwrap();
            }
            CombatImport::of(&rules, &mut names.into_production().into_shells())
        };
        let gun = |name: &str, damage: u8| {
            format!("Weapon {name}\n  PrimaryDamage = {damage}\n  DelayBetweenShots = 100\nEnd\n")
        };
        // Of the same numbers, both soldiers take the one action; the sniper's body, of no most
        // life, gives none.
        let same = import(&(gun("Gun", 5) + gun("GUN", 5).as_str())).unwrap();
        let ids: Vec<&str> = same.actions.keys().map(DeclaredName::as_str).collect();
        assert_eq!(ids, ["weapon_gun"]);
        assert_eq!(same.unliving, ["Sniper"]);
        // Of other numbers, the second, by the types' names, is refused.
        let other = import(&(gun("Gun", 5) + gun("GUN", 6).as_str()));
        assert!(matches!(other, Err(ZeroHourError::WeaponName(name)) if name == "GUN"));
    }

    #[test]
    fn a_living_type_takes_its_armor_by_the_name_of_its_case() {
        let guard = |armor: &str| {
            let text = format!(
                "Object Guard\n  Body = ActiveBody ModuleTag_01\n    MaxHealth = 10\n  End\n  \
                 ArmorSet\n    Conditions = None\n    Armor = {armor}\n  End\nEnd\n"
            );
            let mut rules = ZeroHourRules::default();
            rules.objects.read(text.as_bytes()).unwrap();
            rules.armors.read(ARMORS.as_bytes()).unwrap();
            let mut names = ImportNames::default();
            let object = rules.objects.object(b"Guard").unwrap();
            names.unit_type(object, &LocomotorIni::default()).unwrap();
            let mut names = names.into_production().into_shells();
            CombatImport::of(&rules, &mut names).map(|import| import.armors.len())
        };
        // The fixture's two armors are written whatever the types name; an armor of another
        // case is none the game finds.
        assert_eq!(guard("HumanArmor").unwrap(), 2);
        assert!(matches!(
            guard("humanarmor"),
            Err(ZeroHourError::NoArmor { object, armor }) if object == "Guard" && armor == "humanarmor"
        ));
    }

    #[test]
    fn a_shell_takes_its_objects_name_when_its_weapons_fire_it_alike_and_else_one_of_each() {
        let tanks = |second: &str| {
            format!(
                "Object Gunner\n  WeaponSet\n    Weapon = PRIMARY Cannon\n  End\nEnd\n\
                 Object Tower\n  WeaponSet\n    Weapon = PRIMARY {second}\n  End\nEnd\n"
            )
        };
        let gun = |name: &str, speed: u16| {
            format!(
                "Weapon {name}\n  PrimaryDamage = 60\n  DelayBetweenShots = 100\n  \
                 WeaponSpeed = {speed}\n  ProjectileObject = TankShell\n  \
                 ProjectileCollidesWith = STRUCTURES WALLS\nEnd\n"
            )
        };
        let import = |second: &str, speeds: [u16; 2], extra: &str| {
            let mut rules = ZeroHourRules::default();
            rules.objects.read(DEFAULT_OBJECTS.as_bytes()).unwrap();
            rules.objects.read(OBJECTS.as_bytes()).unwrap();
            rules
                .objects
                .read((tanks(second) + extra).as_bytes())
                .unwrap();
            let text = gun("Cannon", speeds[0]) + gun("Howitzer", speeds[1]).as_str();
            rules.weapons.read(text.as_bytes()).unwrap();
            let mut names = ImportNames::default();
            for object in rules.objects.objects() {
                names.take(object).unwrap();
            }
            for object in ["Gunner", "Tower"] {
                let object = rules.objects.object(object.as_bytes()).unwrap();
                names.unit_type(object, &LocomotorIni::default()).unwrap();
            }
            let mut names = names.into_production().into_shells();
            let combat = CombatImport::of(&rules, &mut names)?;
            let deliveries: Vec<(String, String)> = combat
                .actions
                .iter()
                .map(|(id, weapon)| {
                    let delivery = weapon.delivery.as_ref().unwrap();
                    (
                        id.as_str().to_owned(),
                        delivery.unit_type().as_str().to_owned(),
                    )
                })
                .collect();
            let shells: Vec<(String, ImportedType)> = names
                .types()
                .filter(|(name, ..)| name.as_str().starts_with("tankshell"))
                .map(|(name, _, imported)| (name.as_str().to_owned(), imported.clone()))
                .collect();
            Ok::<_, ZeroHourError>((deliveries, shells, combat.shell_looks))
        };
        let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
        let shell = |name: &str, speed| {
            (
                name.to_owned(),
                ImportedType::shell(shell_projectile(speed)),
            )
        };
        // Two weapons of one speed fire one type, of the object's name, which keeps its look.
        let (deliveries, shells, looks) = import("Howitzer", [400, 400], "").unwrap();
        assert_eq!(
            deliveries,
            [
                pair("weapon_cannon", "tankshell"),
                pair("weapon_howitzer", "tankshell")
            ]
        );
        assert_eq!(shells, [shell("tankshell", 400)]);
        assert_eq!(looks, []);
        // Of two speeds, each fires a type of its own, named for it, which takes the object's
        // look.
        let (deliveries, shells, looks) = import("Howitzer", [400, 300], "").unwrap();
        assert_eq!(
            deliveries,
            [
                pair("weapon_cannon", "tankshell_cannon"),
                pair("weapon_howitzer", "tankshell_howitzer")
            ]
        );
        assert_eq!(
            shells,
            [
                shell("tankshell_cannon", 400),
                shell("tankshell_howitzer", 300)
            ]
        );
        let look = |unit_type: &str| ShellLook {
            unit_type: DeclaredName::new(unit_type).unwrap(),
            object: DeclaredName::new("tankshell").unwrap(),
        };
        assert_eq!(
            looks,
            [look("tankshell_cannon"), look("tankshell_howitzer")]
        );
        let mut client = ClientUnits::default();
        let own = ClientUnit::new(Vec::new());
        client
            .units
            .insert(DeclaredName::new("tankshell").unwrap(), own);
        CombatImport {
            shell_looks: looks,
            ..CombatImport::default()
        }
        .add_shell_looks(&mut client);
        let named: Vec<&str> = client.units.keys().map(DeclaredName::as_str).collect();
        assert_eq!(
            named,
            ["tankshell", "tankshell_cannon", "tankshell_howitzer"]
        );
        // A name of a weapon's shell that an object took is refused.
        let clash = import("Howitzer", [400, 300], "Object TankShell_Cannon\nEnd\n");
        assert!(matches!(
            clash,
            Err(ZeroHourError::NameClash(NameClash { first, second }))
                if first == "TankShell_Cannon" && second == "TankShell"
        ));
    }
}
