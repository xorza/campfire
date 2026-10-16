use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use campfire_math::Num;
use campfire_values::DeclaredName;
use serde::Serialize;

use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::armor_ini::ArmorIni;
use crate::zero_hour::format::ini::damage_type::DamageType;
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::format::ini::weapon_ini::{IniWeapon, ShotDelay, WeaponIni};
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::unit_types::UnitTypes;

/// The slot kind of the converted weapons.
pub(crate) const WEAPON: &str = "weapon";

/// What the combat import reads: the objects, and the weapons and armors they name.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CombatSources<'a> {
    pub(crate) ini: &'a ObjectIni,
    pub(crate) weapons: &'a WeaponIni,
    pub(crate) armors: &'a ArmorIni,
}

/// Zero Hour's combat (04-capabilities/combat.md, Armor by damage kind; 12-zero-hour.md, Life is
/// the original's; 12-zero-hour.md, A weapon converts when it hits one target at once): the weapons
/// of each unit type's starting weapon set that hit one target at once, as attack actions, once for
/// the whole import, each in its type's slot kind `weapon`; every armor, the share of each damage
/// kind it does not take whole; the weapons dropped, each once for each cause, in the order the
/// types are read; the objects whose body the import gives no life, and those whose body starts
/// below its most life; and each type's actions of the slot kind `weapon`, by its name, which it
/// returns.
#[derive(Debug, Default)]
pub(crate) struct CombatImport {
    pub(crate) actions: BTreeMap<DeclaredName, ImportedWeapon>,
    pub(crate) weapons: BTreeMap<ImportName, Vec<DeclaredName>>,
    pub(crate) armors: BTreeMap<DeclaredName, BTreeMap<DeclaredName, Num>>,
    pub(crate) dropped: Vec<DroppedWeapon>,
    pub(crate) unliving: Vec<String>,
    pub(crate) wounded: Vec<String>,
}

/// An attack as the mode's data writes it, `[actions.weapon_<name>]` (12-zero-hour.md, A weapon
/// converts when it hits one target at once).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ImportedWeapon {
    kind: &'static str,
    targeting: &'static str,
    range: Num,
    amount: Num,
    damage_kind: DeclaredName,
    period_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    clip: Option<ImportedClip>,
}

/// A weapon's clip: its shots, and its reload in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct ImportedClip {
    shots: u16,
    reload_ms: u64,
}

/// A weapon the import drops, by its name, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedWeapon {
    pub weapon: String,
    pub cause: WeaponDrop,
}

campfire_common::enum_all! {
/// Why the import drops a weapon (12-zero-hour.md, A weapon converts when it hits one target at
/// once).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WeaponDrop {
    /// No file defines it.
    NoWeapon,
    /// It fires a `ProjectileObject`.
    Projectile,
    /// It damages a `PrimaryDamageRadius` or a `SecondaryDamageRadius`.
    Radius,
    /// It aims within a `ScatterRadius`, or a `ScatterRadiusVsInfantry` at infantry.
    Scatter,
    /// It is no laser, and its `WeaponSpeed` is below the template's default, which the game
    /// holds instant: its damage may land frames after the shot.
    Delayed,
    /// It may not hit the ground, `AntiGround = No`.
    NotGround,
    /// It kills its own unit, `RadiusDamageAffects` of `SUICIDE`.
    Suicide,
    /// It deals its damage at its own place, `DamageDealtAtSelfPosition`.
    AtSelf,
    /// It fires faster as it keeps firing, of a `ContinuousFireOne` or a `ContinuousFireTwo`.
    ContinuousFire,
    /// It waits a `PreAttackDelay` before it strikes.
    PreAttack,
    /// It keeps a `MinimumAttackRange`.
    MinRange,
    /// Its `DelayBetweenShots` is a range the game draws from.
    RandomDelay,
    /// Its `DelayBetweenShots` is no frame, a period the engine refuses.
    NoDelay,
    /// Its `DamageType` takes no life: a status, a subdual, or the kill of a pilot or a garrison.
    NoLife,
    /// Its `DamageType` is `HEALING`, which gives its target life.
    Healing,
    /// A number of it is below 0, not finite, or past what the engine holds.
    BadNumber,
}
/// Every cause, in order.
pub const ALL;
}

/// The game's logic frames a millisecond, `LOGICFRAMES_PER_MSEC_REAL`, in the game's `f32`.
const FRAMES_PER_MS: f32 = 30.0 / 1000.0;

impl CombatImport {
    /// The combat of `sources`, of the closed set `types`, each type's weapons by its name; an
    /// error for a type whose armor no file defines by the name of its case, which the game's
    /// debug build stops at as wrong data and its release build takes as no armor
    /// (`INI::parseArmorTemplate`).
    pub(crate) fn of(sources: CombatSources<'_>, types: &UnitTypes) -> Result<Self, ZeroHourError> {
        let mut import = Self::default();
        let mut armors = BTreeMap::new();
        for (name, armor) in sources.armors.iter() {
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
        for (type_name, typed) in types.iter() {
            let object = sources
                .ini
                .object(typed.object.as_bytes())
                .expect("a typed object is in the file");
            let typed = &typed.imported;
            let fight = object.fight();
            if typed.armor().is_some() {
                let armor = fight
                    .starting_armor()
                    .expect("a type's armor is its object's");
                if sources.armors.get(armor).is_none() {
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
                let converted = sources
                    .weapons
                    .get(weapon)
                    .ok_or(WeaponDrop::NoWeapon)
                    .and_then(ImportedWeapon::of);
                match converted {
                    Ok(imported) => {
                        let id = DeclaredName::new(&format!(
                            "weapon_{}",
                            ImportName::latin(weapon).as_str()
                        ))
                        .expect("a kind and an import name make a declared name");
                        // Weapons whose names differ only in case would share one action.
                        match import.actions.entry(id.clone()) {
                            Entry::Occupied(held) if *held.get() != imported => {
                                return Err(ZeroHourError::WeaponName(weapon.clone()));
                            }
                            Entry::Occupied(_) => {}
                            Entry::Vacant(new) => {
                                new.insert(imported);
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
        Ok(import)
    }
}

impl ImportedWeapon {
    /// The attack of `weapon`; why the import drops it, when it hits no one target at once by the
    /// engine's rules, or holds a number the engine does not.
    fn of(weapon: &IniWeapon) -> Result<Self, WeaponDrop> {
        if weapon.projectile.is_some() {
            return Err(WeaponDrop::Projectile);
        }
        if weapon.radius > 0.0 || weapon.secondary_radius > 0.0 {
            return Err(WeaponDrop::Radius);
        }
        if weapon.scatter > 0.0 || weapon.infantry_scatter > 0.0 {
            return Err(WeaponDrop::Scatter);
        }
        if !weapon.laser && weapon.speed < IniWeapon::INSTANT {
            return Err(WeaponDrop::Delayed);
        }
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
        let period_ms = match Self::ms(delay)? {
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
        let clip = match weapon.clip_size {
            ..0 => return Err(WeaponDrop::BadNumber),
            0 => None,
            size => Some(ImportedClip {
                shots: u16::try_from(size).map_err(|_| WeaponDrop::BadNumber)?,
                reload_ms: Self::ms(weapon.clip_reload_ms)?,
            }),
        };
        Ok(Self {
            kind: "attack",
            targeting: "enemies",
            range: number(weapon.range)?,
            amount: number(weapon.damage)?,
            damage_kind: damage_kind.kind(),
            period_ms,
            clip,
        })
    }

    /// The milliseconds the engine turns into the frames the game turns `ms` into: the game's
    /// `ceil(ms × 0.03)` in its `f32` (`INI::parseDurationUnsignedInt`, `parseShotDelay`), as the
    /// import's rule of times writes it (12-zero-hour.md, Thirty frames a second, the original's
    /// times), `⌊frames × 100 / 3⌋`, which the engine's rounding up turns back into the same
    /// frames; a time below 0 is no number.
    fn ms(ms: i32) -> Result<u64, WeaponDrop> {
        if ms < 0 {
            return Err(WeaponDrop::BadNumber);
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the game's frames, a whole count at least 0 the `f32` holds exactly"
        )]
        let frames = (ms as f32 * FRAMES_PER_MS).ceil() as u64;
        Ok(frames * 100 / 3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::armor_ini::internals::ARMORS;
    use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
    use crate::zero_hour::format::ini::weapon_ini::internals::WEAPONS;
    use crate::zero_hour::import_names::ImportNames;

    #[test]
    fn a_weapon_converts_when_it_hits_one_target_at_once_and_is_dropped_by_its_first_cause() {
        let mut ini = WeaponIni::default();
        ini.read(WEAPONS.as_bytes()).unwrap();
        let of = |name: &str| ImportedWeapon::of(ini.get(name).unwrap());
        // The rifle's 100 ms are ⌈100 × 0.03⌉ = 3 frames, written back as ⌊3 × 100 / 3⌋ = 100;
        // its reload's 700 ms are 21 frames, 700.
        let rifle = ImportedWeapon {
            kind: "attack",
            targeting: "enemies",
            range: Num::int(100),
            amount: Num::int(5),
            damage_kind: DamageType::SmallArms.kind(),
            period_ms: 100,
            clip: Some(ImportedClip {
                shots: 3,
                reload_ms: 700,
            }),
        };
        assert_eq!(of("RangerRifle"), Ok(rifle));
        assert_eq!(of("TankCannon"), Err(WeaponDrop::Projectile));
        assert_eq!(of("Flamer"), Err(WeaponDrop::Radius));
        assert_eq!(of("Knife"), Err(WeaponDrop::PreAttack));
        assert_eq!(of("Mortar"), Err(WeaponDrop::MinRange));
        assert_eq!(of("Pistol"), Err(WeaponDrop::RandomDelay));
        assert_eq!(of("Marker"), Err(WeaponDrop::NoDelay));
        assert_eq!(of("Shotgun"), Err(WeaponDrop::Radius));
        assert_eq!(of("Gatling"), Err(WeaponDrop::Scatter));
        // A speed below the default may land the damage frames after the shot, but a laser's
        // lands at once.
        assert_eq!(of("Bolt"), Err(WeaponDrop::Delayed));
        assert_eq!(of("Stinger"), Err(WeaponDrop::NotGround));
        assert_eq!(of("Blast"), Err(WeaponDrop::Suicide));
        assert_eq!(of("Pulse"), Err(WeaponDrop::AtSelf));
        assert_eq!(of("Minigun"), Err(WeaponDrop::ContinuousFire));
        assert_eq!(of("Disabler"), Err(WeaponDrop::NoLife));
        assert_eq!(of("Mender"), Err(WeaponDrop::Healing));
        assert_eq!(of("Beam").map(|beam| beam.amount), Ok(Num::int(30)));
        // 1 ms is ⌈0.03⌉ = 1 frame, 33 ms; a time below 0 is no number.
        assert_eq!(ImportedWeapon::ms(1), Ok(33));
        assert_eq!(ImportedWeapon::ms(-1), Err(WeaponDrop::BadNumber));
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
        let mut ini = ObjectIni::default();
        ini.read(SOLDIERS.as_bytes()).unwrap();
        let import = |guns: &str| {
            let mut names = ImportNames::default();
            for object in ["Gunner", "Sniper"] {
                let object = ini.object(object.as_bytes()).unwrap();
                names.unit_type(object, &LocomotorIni::default()).unwrap();
            }
            let types = names.close();
            let mut weapons = WeaponIni::default();
            weapons.read(guns.as_bytes()).unwrap();
            let sources = CombatSources {
                ini: &ini,
                weapons: &weapons,
                armors: &ArmorIni::default(),
            };
            CombatImport::of(sources, &types)
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
            let mut ini = ObjectIni::default();
            ini.read(text.as_bytes()).unwrap();
            let mut names = ImportNames::default();
            let object = ini.object(b"Guard").unwrap();
            names.unit_type(object, &LocomotorIni::default()).unwrap();
            let mut armors = ArmorIni::default();
            armors.read(ARMORS.as_bytes()).unwrap();
            let sources = CombatSources {
                ini: &ini,
                weapons: &WeaponIni::default(),
                armors: &armors,
            };
            CombatImport::of(sources, &names.close()).map(|import| import.armors.len())
        };
        // The fixture's two armors are written whatever the types name; an armor of another
        // case is none the game finds.
        assert_eq!(guard("HumanArmor").unwrap(), 2);
        assert!(matches!(
            guard("humanarmor"),
            Err(ZeroHourError::NoArmor { object, armor }) if object == "Guard" && armor == "humanarmor"
        ));
    }
}
