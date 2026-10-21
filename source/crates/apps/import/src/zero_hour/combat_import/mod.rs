use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use std::num::NonZeroU8;

use campfire_actions::{ActionKeys, ActionKind, ActionRange, DeliveryData, RangeField, Targeting};
use campfire_client_files::ClientUnits;
use campfire_codec::{TomlTable, TomlValue};
use campfire_combat::{
    ArmorShare, AttackData, ClipData, OuterData, PitchData, ScatterData, SplashData, StageData,
    StreakData, WeaponValue,
};
use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_projectiles::ProjectileData;
use campfire_values::{FilterData, Number, Ranked, RelationSet, Scalar, Speed, Stat};

use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::damage_type::DamageType;
use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
use crate::zero_hour::format::ini::object_ini::{IniObject, ObjectIni};
use crate::zero_hour::format::ini::weapon_ini::{IniWeapon, RadiusAffects, ShotDelay};
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ProjectileNames;
use crate::zero_hour::imported_type::{INFANTRY, ImportedType};
use crate::zero_hour::object_flight::ObjectFlight;
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
/// its name, which it returns; the projectile types of a weapon's own, each with the object
/// whose look it takes; and the modifiers of the weapons' stages of continuous fire.
#[derive(Debug, Default)]
pub(crate) struct CombatImport {
    pub(crate) actions: BTreeMap<DeclaredName, ImportedWeapon>,
    pub(crate) modifiers: BTreeMap<DeclaredName, StageModifier>,
    pub(crate) weapons: BTreeMap<ImportName, Vec<DeclaredName>>,
    pub(crate) armors: BTreeMap<DeclaredName, BTreeMap<DeclaredName, ArmorShare>>,
    pub(crate) dropped: Vec<DroppedWeapon>,
    pub(crate) unliving: Vec<String>,
    pub(crate) wounded: Vec<String>,
    projectile_looks: Vec<ProjectileLook>,
}

/// A projectile type named for its weapon, and the object whose look it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectileLook {
    unit_type: DeclaredName,
    object: DeclaredName,
}

/// An attack as the mode's data writes it, `[actions.weapon_<name>]` (09-zero-hour.md, A weapon
/// converts when it hits one target at once): its numbers, its clip, the projectile it fires,
/// the damage of its radius, its scatter, the pitches it reaches, and the speed of a weapon of
/// no projectile whose damage lands after its shot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedWeapon {
    pub(crate) keys: ActionKeys,
    pub(crate) attack: AttackData,
}

/// A weapon the import converts, the projectile it fires, if it fires one, and the modifiers of
/// its stages of continuous fire, by their names.
#[derive(Debug, Clone)]
struct ConvertedWeapon<'a> {
    weapon: ImportedWeapon,
    fired: Option<FiredProjectile<'a>>,
    modifiers: Vec<(DeclaredName, StageModifier)>,
}

/// A stage of a weapon's continuous fire as the mode's data writes it, `[modifiers.<weapon>_mean]`
/// or `_fast`: what it adds to the rate of fire, the stage's bonus less 1, as
/// `WeaponBonus::appendBonuses` adds it (09-zero-hour.md, Continuous fire
/// converts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StageModifier {
    pub(crate) stats: BTreeMap<Stat, Number>,
}

impl StageModifier {
    /// Its table under `[modifiers]`.
    pub(crate) fn table(&self) -> TomlValue {
        TomlValue::of(&BTreeMap::from([("stats", &self.stats)]))
    }
}

/// The stat a stage of continuous fire adds to, which the rules package declares.
pub(crate) const RATE_OF_FIRE: &str = "rate_of_fire";

/// The names of a weapon's stages of continuous fire, after `CONTINUOUS_FIRE_MEAN` and
/// `CONTINUOUS_FIRE_FAST`.
const STAGES: [&str; 2] = ["mean", "fast"];

/// What a weapon's conversion reads past the weapon: the objects, whose projectile it fires, and
/// the locomotors, which fly a missile.
#[derive(Debug, Clone, Copy)]
struct WeaponRules<'a> {
    objects: &'a ObjectIni,
    locomotors: &'a LocomotorIni,
}

/// The object a weapon fires, and its projectile as that weapon fires it.
#[derive(Debug, Clone)]
struct FiredProjectile<'a> {
    object: &'a IniObject,
    projectile: ProjectileData,
}

/// A weapon the import drops, by its name, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedWeapon {
    pub weapon: String,
    pub cause: WeaponDrop,
}

/// A weapon's streak of continuous fire, and the modifiers of its stages, by their names.
#[derive(Debug, Clone)]
struct Stages {
    streak: StreakData,
    modifiers: Vec<(DeclaredName, StageModifier)>,
}

/// A quarter turn, the steepest pitch, in degrees.
const QUARTER_DEG: f32 = 90.0;

impl CombatImport {
    /// The combat of `rules`, of the unit types `names` made, each type's weapons by its name;
    /// the projectile types its weapons fire join `names`. An error for a type whose armor no file
    /// defines by the name of its case, which the game's debug build stops at as wrong data and
    /// its release build takes as no armor (`INI::parseArmorTemplate`), and for a projectile
    /// type's name another object took.
    pub(crate) fn of(
        rules: &ZeroHourRules,
        names: &mut ProjectileNames,
    ) -> Result<Self, ZeroHourError> {
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
        // Each converted weapon's projectile, by its action and with its weapon's name.
        let mut fired = BTreeMap::<DeclaredName, (&str, FiredProjectile<'_>)>::new();
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
                let id =
                    DeclaredName::new(&format!("weapon_{}", ImportName::latin(weapon).as_str()))
                        .expect("a kind and an import name make a declared name");
                let converted = rules
                    .weapons
                    .get(weapon)
                    .ok_or(WeaponDrop::NoWeapon)
                    .and_then(|ini| {
                        let rules = WeaponRules {
                            objects: &rules.objects,
                            locomotors: &rules.locomotors,
                        };
                        ImportedWeapon::of(ini, rules, &id)
                    });
                match converted {
                    Ok(converted) => {
                        // Weapons whose names differ only in case would share one action.
                        match import.actions.entry(id.clone()) {
                            Entry::Occupied(held)
                                if *held.get() != converted.weapon
                                    || !FiredProjectile::alike(
                                        fired.get(&id).map(|(_, held)| held),
                                        converted.fired.as_ref(),
                                    ) =>
                            {
                                return Err(ZeroHourError::WeaponName(weapon.clone()));
                            }
                            Entry::Occupied(_) => {}
                            Entry::Vacant(new) => {
                                new.insert(converted.weapon);
                                if let Some(projectile) = converted.fired {
                                    fired.insert(id.clone(), (weapon.as_str(), projectile));
                                }
                                import.modifiers.extend(converted.modifiers);
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
        import.name_projectiles(fired, names)?;
        Ok(import)
    }

    /// Names the projectile type of each of `fired`, by its action, in `names`, and delivers it from
    /// its action: an object's own name when every weapon that fires it fires it alike, else for
    /// each weapon one of the object's name and the weapon's, which takes the object's look.
    fn name_projectiles(
        &mut self,
        fired: BTreeMap<DeclaredName, (&str, FiredProjectile<'_>)>,
        names: &mut ProjectileNames,
    ) -> Result<(), ZeroHourError> {
        let mut by_object = BTreeMap::<&str, Vec<(DeclaredName, &str, FiredProjectile<'_>)>>::new();
        for (action, (weapon, projectile)) in fired {
            by_object
                .entry(projectile.object.name.as_str())
                .or_default()
                .push((action, weapon, projectile));
        }
        for group in by_object.into_values() {
            let alike = group
                .windows(2)
                .all(|pair| pair[0].2.projectile == pair[1].2.projectile);
            let mut own_type: Option<DeclaredName> = None;
            for (action, weapon, FiredProjectile { object, projectile }) in group {
                let own = ImportName::object(object);
                let unit_type = if let Some(named) = &own_type {
                    named.clone()
                } else if alike {
                    let named =
                        names.projectile(own, object, ImportedType::of_projectile(projectile))?;
                    own_type.insert(named).clone()
                } else {
                    let name = ImportName::latin(&format!("{}_{weapon}", object.name));
                    let named =
                        names.projectile(name, object, ImportedType::of_projectile(projectile))?;
                    self.projectile_looks.push(ProjectileLook {
                        unit_type: named.clone(),
                        object: own.declared(),
                    });
                    named
                };
                let held = self
                    .actions
                    .get_mut(&action)
                    .expect("a projectile's action is converted");
                held.keys.delivery = Some(DeliveryData::Projectile {
                    unit_type,
                    count: NonZeroU8::MIN,
                    spread_deg: Num::ZERO,
                });
            }
        }
        Ok(())
    }

    /// Gives each projectile type named for its weapon, in `looks`, the look of the object it fires,
    /// when that object has one.
    pub(crate) fn add_projectile_looks(&self, looks: &mut ClientUnits) {
        for ProjectileLook { unit_type, object } in &self.projectile_looks {
            if let Some(look) = looks.units.get(object) {
                let look = look.clone();
                looks.units.insert(unit_type.clone(), look);
            }
        }
    }
}

impl FiredProjectile<'_> {
    /// Whether `a` and `b` are one object fired as one projectile, or both none.
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

    /// The attack `id` of `weapon`, by `rules`, the projectile it fires and the modifiers of its
    /// stages of continuous fire; why the import drops it, when it hits no target by the
    /// engine's rules, or holds a number the engine does not.
    fn of<'a>(
        weapon: &IniWeapon,
        rules: WeaponRules<'a>,
        id: &DeclaredName,
    ) -> Result<ConvertedWeapon<'a>, WeaponDrop> {
        let object = match &weapon.projectile {
            Some(_) if weapon.stream => return Err(WeaponDrop::Stream),
            Some(name) => Some(
                rules
                    .objects
                    .object(name.as_bytes())
                    .and_then(|object| Some((object, ObjectFlight::of(object, rules.locomotors)?)))
                    .ok_or(WeaponDrop::NoFlight)?,
            ),
            None => None,
        };
        if weapon.detonation_objects {
            return Err(WeaponDrop::DetonationObjects);
        }
        let splashes = weapon.radius > 0.0 || weapon.secondary_radius > 0.0;
        if splashes && object.is_none() {
            return Err(WeaponDrop::Radius);
        }
        let scatters = weapon.scatter > 0.0 || weapon.infantry_scatter > 0.0;
        if scatters && object.is_none() {
            return Err(WeaponDrop::Scatter);
        }
        let affects = splashes
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
        let stages = Self::stages(weapon, id)?;
        if weapon.pre_attack_ms > 0 {
            return Err(WeaponDrop::PreAttack);
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
        // Damage within a secondary radius past the primary is an outer ring; one no farther
        // than the primary deals nothing beyond it, as each unit within the primary takes the
        // primary damage alone (`dealDamageInternal`).
        let splash = match affects {
            Some(affects) => {
                let radius = number(weapon.radius)?;
                let splash = SplashData::new(radius, affects, !weapon.affects.source)
                    .ok_or(WeaponDrop::BadNumber)?;
                if weapon.secondary_radius > weapon.radius {
                    let amount = Number::Value(Scalar::Decimal(number(weapon.secondary_damage)?));
                    let outer =
                        OuterData::new(number(weapon.secondary_radius)?, Ranked::One(amount));
                    Some(splash.with_outer(outer).ok_or(WeaponDrop::BadNumber)?)
                } else {
                    Some(splash)
                }
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
        // A laser's damage lands at once, and a projectile's when it ends.
        let speed = if object.is_none() && !weapon.laser && weapon.speed < IniWeapon::INSTANT {
            Some(Speed::new(number(weapon.speed)?).ok_or(WeaponDrop::BadNumber)?)
        } else {
            None
        };
        let fired = match object {
            Some((object, flight)) => Some(FiredProjectile {
                object,
                projectile: flight.projectile(weapon, object)?,
            }),
            None => None,
        };
        let damage = Ranked::One(Number::Value(Scalar::Decimal(number(weapon.damage)?)));
        let meters = number(weapon.range)?;
        let range = RangeField::Range(ActionRange::meters(meters));
        let min_range = if weapon.min_range > 0.0 {
            let min = number(weapon.min_range)?;
            if min >= meters {
                return Err(WeaponDrop::MinRange);
            }
            Some(Ranked::One(Number::Value(Scalar::Decimal(min))))
        } else {
            None
        };
        let enemies = FilterData::parse("enemies").expect("the enemies' filter parses");
        Ok(ConvertedWeapon {
            weapon: Self {
                keys: ActionKeys {
                    range: Some(Ranked::One(range)),
                    min_range,
                    ..ActionKeys::new(ActionKind::Attack, Targeting::Unit(enemies))
                },
                attack: AttackData {
                    rate: WeaponValue::Own(ms(period_ms)?),
                    rate_bonus: stages
                        .as_ref()
                        .map(|_| Stat::named(RATE_OF_FIRE).expect("a stat's name")),
                    damage: WeaponValue::Own(damage),
                    damage_kind: damage_kind.kind(),
                    clip,
                    splash,
                    scatter,
                    pitch_deg,
                    speed,
                    streak: stages.as_ref().map(|stages| stages.streak.clone()),
                },
            },
            fired,
            modifiers: stages.map_or_else(Vec::new, |stages| stages.modifiers),
        })
    }

    /// The streak of `weapon`'s continuous fire, of the action `id`, and the modifiers of its two
    /// stages, named after it, above `ContinuousFireOne` and `ContinuousFireTwo` shots, each of
    /// its stage's rate of fire, which adds 0 for a stage of no bonus; none for a weapon whose
    /// stages change no rate, as they change nothing. Dropped for a stage's bonus to another
    /// field, or a coast of no time, which the engine does not hold, and for shots below 0 or
    /// not growing (09-zero-hour.md, Continuous fire converts).
    fn stages(weapon: &IniWeapon, id: &DeclaredName) -> Result<Option<Stages>, WeaponDrop> {
        let fire = weapon.continuous_fire;
        if fire.changes_other_fields() {
            return Err(WeaponDrop::ContinuousFire);
        }
        let rates = fire.rates();
        if rates == [1.0; 2] {
            return Ok(None);
        }
        let [one, two] = fire
            .shots
            .map(|shots| u32::try_from(shots).map_err(|_| WeaponDrop::BadNumber));
        let shots = [one?, two?];
        if shots[0] >= shots[1] {
            return Err(WeaponDrop::BadNumber);
        }
        let coast_ms = match weapon_drop::game_ms(fire.coast_ms)? {
            0 => return Err(WeaponDrop::ContinuousFire),
            ms => ms,
        };
        let stat = Stat::named(RATE_OF_FIRE).expect("a stat's name");
        let mut modifiers = Vec::with_capacity(STAGES.len());
        let mut stages = Vec::with_capacity(STAGES.len());
        for ((stage, shots), rate) in STAGES.into_iter().zip(shots).zip(rates) {
            let name = DeclaredName::new(&format!("{id}_{stage}"))
                .expect("an action's name and a stage's make a declared name");
            // `WeaponBonus::appendBonuses` adds a bonus less 1, in the game's `f32`.
            let added = game_float::to_num(rate - 1.0).map_err(|_| WeaponDrop::BadNumber)?;
            let change = Number::Value(Scalar::Decimal(added));
            modifiers.push((
                name.clone(),
                StageModifier {
                    stats: BTreeMap::from([(stat.clone(), change)]),
                },
            ));
            stages.push(StageData {
                shots,
                modifier: name,
            });
        }
        Ok(Some(Stages {
            streak: StreakData { coast_ms, stages },
            modifiers,
        }))
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
