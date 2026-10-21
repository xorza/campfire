use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use std::num::NonZeroU8;

use campfire_actions::{ActionKeys, ActionKind, ActionRange, DeliveryData, RangeField, Targeting};
use campfire_client_files::ClientUnits;
use campfire_codec::TomlTable;
use campfire_combat::{
    ArmorShare, AttackData, ClipData, PitchData, ScatterData, SplashData, WeaponValue,
};
use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_projectiles::ProjectileData;
use campfire_values::{FilterData, Number, Ranked, RelationSet, Scalar, Speed};

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

/// Zero Hour's combat (04-capabilities/combat.md, Armor by damage kind; 09-zero-hour.md, Life is
/// the original's; 09-zero-hour.md, A weapon converts when it hits one target at once;
/// 09-zero-hour.md, The import converts shells, splash, scatter, pitch, turrets and
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
    pub(crate) armors: BTreeMap<DeclaredName, BTreeMap<DeclaredName, ArmorShare>>,
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

/// An attack as the mode's data writes it, `[actions.weapon_<name>]` (09-zero-hour.md, A weapon
/// converts when it hits one target at once): its numbers, its clip, the shell it fires, the
/// damage of its radius, its scatter, the pitches it reaches, and the speed of a weapon of no
/// shell whose damage lands after its shot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedWeapon {
    pub(crate) keys: ActionKeys,
    pub(crate) attack: AttackData,
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
                    .and_then(ArmorShare::new)
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
                held.keys.delivery = Some(DeliveryData::Projectile {
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
    /// Its table under `[actions]`.
    pub(crate) fn table(&self) -> TomlTable {
        self.keys.table(&self.attack)
    }

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
                keys: ActionKeys {
                    range: Some(Ranked::One(range)),
                    ..ActionKeys::new(ActionKind::Attack, Targeting::Unit(enemies))
                },
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
mod tests;
