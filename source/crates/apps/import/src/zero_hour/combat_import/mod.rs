use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use campfire_actions::{ActionKeys, ActionRange, RangeField, Targeting};
use campfire_client_files::ClientUnit;
use campfire_codec::{TomlTable, TomlValue};
use campfire_combat::{
    ArmorShare, AttackData, BarrelsData, BonusData, ClipData, ClipRefill, FallData, OuterData,
    PitchData, PreAttackData, PreAttackEach, ScatterData, SplashData, StageData, StreakData,
    WeaponValue,
};
use campfire_common::DeclaredName;
use campfire_math::{Num, Rounding};
use campfire_projectiles::{ProjectileData, ProjectileDeliveryData};
use campfire_values::{FilterTag, Number, Ranked, RelationSet, Scalar, Speed, Stat, TagFilterData};
use serde::Serialize;

use crate::zero_hour::combat_import::converted_shot::{ConvertedShot, WeaponShot};
use crate::zero_hour::dropped::Losses;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::damage_type::DamageType;
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::format::ini::object_ini::fight_data::{PreferredKinds, WeaponSet};
use crate::zero_hour::format::ini::weapon_ini::{IniWeapon, PrefireType, RadiusAffects, ShotDelay};
use crate::zero_hour::game_time;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::{ProjectileNames, TypedObject};
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::model_bones::ModelBones;
use crate::zero_hour::object_flight::ObjectFlight;
use crate::zero_hour::portable_structure;
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::type_name::TypeName;
use crate::zero_hour::weapon_drop_cause::WeaponDropCause;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

pub(crate) mod converted_shot;
mod weapon_gate;

use crate::zero_hour::combat_import::weapon_gate::{GatedWeapon, WeaponGate};

/// Zero Hour's combat (04-capabilities/combat.md, Armor by damage kind; 09-zero-hour.md, Life is
/// the original's; 09-zero-hour.md, A weapon converts when it hits one target at once;
/// 09-zero-hour.md, The import converts shells, splash, scatter, pitch, turrets and slow weapons;
/// 09-zero-hour.md, Zero Hour's upgrade flags are tags and stats): the weapons of each unit type's
/// starting weapon set and of its set of `PLAYER_UPGRADE` that convert, as attack actions, once for
/// the whole import and its gate between the sets, each in its type's slot kind `weapon`; the stats
/// their upgrades' damage bonuses name, each with what a `WeaponBonusUpgrade` adds to it; every
/// armor, the share of each damage kind it does not take whole; the weapons dropped, each once for
/// each cause, in the order the types are read; the objects whose body the import gives no life,
/// and those whose body starts below its most life; each type's actions of the slot kind `weapon`,
/// by its name, which it returns, and the same actions by the place of their slot in its sets,
/// which its look's attack conditions test; the projectile types of a weapon's own, each with the
/// object whose look it takes; and the modifiers of the weapons' stages of continuous fire.
#[derive(Debug, Default)]
pub(crate) struct CombatImport {
    pub(crate) actions: BTreeMap<DeclaredName, ImportedWeapon>,
    /// The game's weapon each action converts, by the action.
    sources: BTreeMap<DeclaredName, String>,
    pub(crate) modifiers: BTreeMap<DeclaredName, StageModifier>,
    pub(crate) weapons: BTreeMap<TypeName, Vec<DeclaredName>>,
    pub(crate) weapon_slots: BTreeMap<TypeName, Vec<Vec<DeclaredName>>>,
    /// The types a weapon of their starting set arms.
    armed: BTreeSet<TypeName>,
    pub(crate) bonuses: BTreeMap<Stat, Number>,
    pub(crate) armors: BTreeMap<DeclaredName, BTreeMap<DeclaredName, ArmorShare>>,
    pub(crate) dropped: Losses<WeaponDropCause>,
    pub(crate) unliving: Vec<String>,
    pub(crate) wounded: Vec<String>,
    /// The weapons whose `PreferredAgainst` names a kind of no tag, whose preference goes.
    pub(crate) unpreferred: BTreeSet<String>,
    projectile_looks: Vec<ProjectileLook>,
}

/// The weapons of a portable structure its carrier holds: the structure's name, which each of its
/// actions takes after the weapon's, and the tag of the upgrade that makes it, which arms them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CarriedWeapons {
    by: DeclaredName,
    tag: DeclaredName,
}

impl CarriedWeapons {
    /// The action of the weapon of the action `id` it carries: `id` suffixed by the structure's
    /// name; a long name for one past the bytes a name holds.
    fn id(&self, id: &DeclaredName) -> Result<DeclaredName, WeaponDropCause> {
        let name = ImportName::of(id.as_str().as_bytes());
        name.suffixed(self.by.as_str())
            .map_err(|_| WeaponDropCause::LongName)
    }

    /// The test of its carrier's tags each of its actions takes: the upgrade's tag.
    fn when(&self) -> TagFilterData {
        TagFilterData::new([FilterTag {
            name: self.tag.clone(),
            negated: false,
        }])
    }
}

/// A projectile type named for its weapon, and the object whose look it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectileLook {
    unit_type: TypeName,
    object: TypeName,
}

/// An attack as the mode's data writes it, `[actions.weapon_<name>]` (09-zero-hour.md, A weapon
/// converts when it hits one target at once): its numbers, its clip, the projectile it fires,
/// the damage of its radius, its scatter, the pitches it reaches, and the speed of a weapon of
/// no projectile whose damage lands after its shot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedWeapon {
    pub(crate) keys: ActionKeys,
    pub(crate) attack: AttackData,
    /// The projectile it fires, when it fires one.
    pub(crate) delivery: Option<ProjectileDeliveryData>,
}

/// A weapon the import converts, the projectile it fires, if it fires one, the modifiers of its
/// stages of continuous fire, by their names, and its upgrade's damage bonus.
#[derive(Debug, Clone)]
struct ConvertedWeapon<'a> {
    weapon: ImportedWeapon,
    fired: Option<FiredProjectile<'a>>,
    modifiers: Vec<(DeclaredName, StageModifier)>,
    bonus: Option<DamageBonus>,
}

/// The stat a weapon's damage bonus of `PLAYER_UPGRADE` names, `upgrade_damage_<percent>`, one for
/// each percent of the install's, and what a `WeaponBonusUpgrade` adds to it: the bonus less 1, as
/// `WeaponBonus::appendBonuses` adds it, in the game's `f32` (09-zero-hour.md, Zero Hour's upgrade
/// flags are tags and stats).
#[derive(Debug, Clone, PartialEq, Eq)]
struct DamageBonus {
    stat: Stat,
    added: Number,
}

/// A stage of a weapon's continuous fire, or its fall, as the mode's data writes it,
/// `[modifiers.<weapon>_mean]`, `_fast` or `_slow`: what it adds to the rate of fire, a stage's
/// bonus less 1, as `WeaponBonus::appendBonuses` adds it, none for the fall; and the tag of its
/// condition (09-zero-hour.md, Continuous fire converts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct StageModifier {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) stats: BTreeMap<Stat, Number>,
    pub(crate) tags: Vec<DeclaredName>,
}

impl StageModifier {
    /// Its table under `[modifiers]`.
    pub(crate) fn table(&self) -> TomlValue {
        TomlValue::of(self)
    }
}

/// The names of a weapon's stages of continuous fire, after `CONTINUOUS_FIRE_MEAN` and
/// `CONTINUOUS_FIRE_FAST`.
const STAGES: [&str; 2] = ["mean", "fast"];

/// The name of a weapon's fall from a stage, after `CONTINUOUS_FIRE_SLOW`.
const FALL: &str = "slow";

/// The frames a fall past the coast holds, `LOGICFRAMES_PER_SECOND`, as `FiringTracker::update`
/// sets its next cool-down a second on.
const FALL_FRAMES: u64 = 30;

/// The object a weapon fires, and its projectile as that weapon fires it.
#[derive(Debug, Clone)]
struct FiredProjectile<'a> {
    object: &'a IniObject,
    projectile: ProjectileData,
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
    /// Whether a weapon it converted arms `unit` as it starts, before any upgrade: one of its
    /// starting set, as a unit armed by its upgrade set alone attacks nothing until its upgrade.
    pub(crate) fn arms(&self, unit: &DeclaredName) -> bool {
        self.armed.contains(unit)
    }

    /// The combat of `rules`, of the unit types `names` made, each type's weapons by its name;
    /// the projectile types its weapons fire join `names`. An error for a type whose armor no file
    /// defines by the name of its case, which the game's debug build stops at as wrong data and
    /// its release build takes as no armor (`INI::parseArmorTemplate`), and for a projectile
    /// type's name another object took.
    pub(crate) fn of<'r>(
        rules: &'r ZeroHourRules,
        names: &mut ProjectileNames<'r>,
    ) -> Result<Self, ZeroHourError> {
        let mut import = Self::default();
        let mut armors = BTreeMap::new();
        for (name, armor) in rules.armors.iter() {
            let id = ImportName::latin(name).declared()?;
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
        for TypedObject {
            name: type_name,
            object,
            imported: typed,
        } in names.types()
        {
            let fight = object.fight();
            // A type of an armor lives, and its armors are its object's starting and upgrade
            // ones.
            let armors = [fight.starting_armor(), fight.upgrade_armor()].into_iter();
            if typed.armor().is_some() || !typed.tagged_armors().is_empty() {
                for armor in armors.flatten() {
                    if rules.armors.get(armor).is_none() {
                        return Err(ZeroHourError::NoArmor {
                            object: object.name.clone(),
                            armor: armor.to_owned(),
                        });
                    }
                }
            }
            match (fight.body, typed.life()) {
                (Some(body), Some(_)) if body.initial < body.max => {
                    import.wounded.push(object.name.clone());
                }
                (Some(_), None) if !typed.leads() => import.unliving.push(object.name.clone()),
                _ => {}
            }
            // A leader of spawns fights by them: its own weapon, as a mob's harmless one, goes
            // (09-zero-hour.md, The import converts a mob). A portable structure an upgrade makes
            // inside it adds its starting set, of the slots past its own, armed by the upgrade's
            // tag (riders-and-upgrades.md, A portable structure is a part of its carrier).
            let own = fight.starting_weapons().filter(|_| !typed.leads());
            let mut sources = Vec::new();
            if let Some(set) = own {
                let gated = WeaponGate::of(set, fight.upgrade_weapons());
                sources.push((gated, None));
            }
            for module in object.upgrades() {
                let Some(structure) = portable_structure::structure(rules, module) else {
                    continue;
                };
                let set = structure.fight().starting_weapons();
                let tag = portable_structure::tag(module)?;
                let (Some(set), Some(tag)) = (set, tag) else {
                    continue;
                };
                let carried = CarriedWeapons {
                    by: ImportName::latin(&structure.name).declared()?,
                    tag,
                };
                sources.push((WeaponGate::of(set, None), Some(carried)));
            }
            let gated = sources
                .iter()
                .enumerate()
                .flat_map(|(at, (gated, carried))| {
                    gated
                        .iter()
                        .map(move |gated| (WeaponSet::SLOTS * at, gated, carried.as_ref()))
                });
            for (
                offset,
                &GatedWeapon {
                    slot,
                    weapon,
                    gate,
                    set,
                },
                carried,
            ) in gated
            {
                if set.locked[slot] {
                    import.dropped.drop(weapon, WeaponDropCause::Locked);
                    continue;
                }
                let named = ImportName::latin(weapon).id("weapon")?;
                let converted = rules
                    .weapons
                    .get(weapon)
                    .ok_or(WeaponDropCause::NoWeapon)
                    .and_then(|ini| ImportedWeapon::of(ini, rules, &named))
                    .and_then(|converted| {
                        let id = match carried {
                            Some(carried) => carried.id(&named)?,
                            None => gate.id(&named)?,
                        };
                        Ok((id, converted))
                    })
                    .and_then(|(id, converted)| {
                        import.take_bonus(converted.bonus.as_ref())?;
                        Ok((id, converted))
                    });
                match converted {
                    Ok((id, mut converted)) => {
                        converted.weapon.keys.when = match carried {
                            Some(carried) => Some(carried.when()),
                            None => gate.when(&rules.names),
                        };
                        if let Some(kinds) = set.preferred[slot] {
                            converted.weapon.attack.prefers = Self::preference(kinds, &rules.names);
                            if kinds.other {
                                import.unpreferred.insert(weapon.to_owned());
                            }
                        }
                        // Weapons whose names differ only in case would share one action.
                        match import.actions.entry(id.clone()) {
                            Entry::Occupied(held)
                                if *held.get() != converted.weapon
                                    || !FiredProjectile::alike(
                                        fired.get(&id).map(|(_, held)| held),
                                        converted.fired.as_ref(),
                                    ) =>
                            {
                                return Err(ZeroHourError::WeaponName(weapon.to_owned()));
                            }
                            Entry::Occupied(_) => {}
                            Entry::Vacant(new) => {
                                new.insert(converted.weapon);
                                import.sources.insert(id.clone(), weapon.to_owned());
                                if let Some(projectile) = converted.fired {
                                    fired.insert(id.clone(), (weapon, projectile));
                                }
                                import.modifiers.extend(converted.modifiers);
                            }
                        }
                        let slots = import.weapon_slots.entry(type_name.clone()).or_default();
                        let place = offset + slot;
                        if slots.len() <= place {
                            slots.resize_with(place + 1, Vec::new);
                        }
                        if !slots[place].contains(&id) {
                            slots[place].push(id.clone());
                        }
                        let weapons = import.weapons.entry(type_name.clone()).or_default();
                        if !weapons.contains(&id) {
                            weapons.push(id);
                        }
                        if gate != WeaponGate::Upgraded && carried.is_none() {
                            import.armed.insert(type_name.clone());
                        }
                    }
                    Err(cause) => import.dropped.drop(weapon, cause),
                }
            }
        }
        import.armors = armors;
        import.name_projectiles(fired, names)?;
        Ok(import)
    }

    /// The filter of the targets of all of `kinds`, a weapon set's `PreferredAgainst`, by their
    /// tags of `names`; none for a kind of no tag, as `BALLISTIC_MISSILE`, which no unit of the
    /// engine's is (09-zero-hour.md, The import converts what the choice and the answer read).
    fn preference(kinds: PreferredKinds, names: &RulesNames) -> Option<TagFilterData> {
        let PreferredKinds {
            infantry,
            vehicle,
            structure,
            aircraft,
            other,
        } = kinds;
        let tags = [
            (infantry, &names.infantry),
            (vehicle, &names.vehicle),
            (structure, &names.structure),
            (aircraft, &names.aircraft),
        ];
        let tags = tags
            .into_iter()
            .filter(|&(holds, _)| holds)
            .map(|(_, tag)| FilterTag {
                name: tag.clone(),
                negated: false,
            });
        (!other).then(|| TagFilterData::new(tags))
    }

    /// Keeps the stat of `bonus`, a weapon's, with what an upgrade adds to it; a bad number for a
    /// stat another weapon's bonus of another percent named, as two percents round to one name.
    fn take_bonus(&mut self, bonus: Option<&DamageBonus>) -> Result<(), WeaponDropCause> {
        let Some(DamageBonus { stat, added }) = bonus else {
            return Ok(());
        };
        match self.bonuses.entry(stat.clone()) {
            Entry::Occupied(held) if held.get() != added => Err(WeaponDropCause::BadNumber),
            Entry::Occupied(_) => Ok(()),
            Entry::Vacant(new) => {
                new.insert(added.clone());
                Ok(())
            }
        }
    }

    /// Names the projectile type of each of `fired`, by its action, in `names`, and delivers it
    /// from its action: an object's own name when every weapon that fires it fires it alike, else
    /// for each weapon one of the object's name and the weapon's, which takes the object's look.
    fn name_projectiles<'r>(
        &mut self,
        fired: BTreeMap<DeclaredName, (&str, FiredProjectile<'r>)>,
        names: &mut ProjectileNames<'r>,
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
            let mut own_type: Option<TypeName> = None;
            for (action, weapon, FiredProjectile { object, projectile }) in group {
                let own = TypeName::of(object)?;
                let unit_type = if let Some(named) = &own_type {
                    named.clone()
                } else if alike {
                    let named =
                        names.projectile(own, object, ImportedType::of_projectile(projectile))?;
                    own_type.insert(named).clone()
                } else {
                    let name =
                        TypeName::new(&ImportName::latin(&format!("{}_{weapon}", object.name)))?;
                    let named =
                        names.projectile(name, object, ImportedType::of_projectile(projectile))?;
                    self.projectile_looks.push(ProjectileLook {
                        unit_type: named.clone(),
                        object: own,
                    });
                    named
                };
                let held = self
                    .actions
                    .get_mut(&action)
                    .expect("a projectile's action is converted");
                held.delivery = Some(ProjectileDeliveryData::one(unit_type.into()));
            }
        }
        Ok(())
    }

    /// Gives each projectile type named for its weapon, in the unit looks `looks`, the look of the
    /// object it fires, when that object has one; then each type's look its weapons by the places
    /// of their slots, which its attack conditions test (09-zero-hour.md, A condition is the game's
    /// name for a test).
    pub(crate) fn complete_looks(&self, looks: &mut BTreeMap<DeclaredName, ClientUnit>) {
        for ProjectileLook { unit_type, object } in &self.projectile_looks {
            if let Some(look) = looks.get(object.declared()) {
                let look = look.clone();
                looks.insert(unit_type.declared().clone(), look);
            }
        }
        for (unit_type, slots) in &self.weapon_slots {
            if let Some(look) = looks.get_mut(unit_type.declared()) {
                look.weapons.clone_from(slots);
            }
        }
    }

    /// Each action it converts, and how it shoots, by the game's weapon it converts, one weapon to
    /// an action of each set it serves (09-zero-hour.md, A look follows its converted
    /// action).
    pub(crate) fn converted_shots(&self) -> BTreeMap<&str, Vec<ConvertedShot<'_>>> {
        let mut shots: BTreeMap<&str, Vec<ConvertedShot<'_>>> = BTreeMap::new();
        for (action, weapon) in &self.sources {
            let shot = match &self.actions[action].delivery {
                Some(delivery) => WeaponShot::Projectile(&delivery.projectile),
                None => WeaponShot::Shot,
            };
            shots
                .entry(weapon.as_str())
                .or_default()
                .push(ConvertedShot { action, shot });
        }
        shots
    }

    /// The barrels of each type's weapons that fire a projectile: of each weapon slot of the type,
    /// the barrels its model's bones give the slot, as Zero Hour launches the slot's weapon of
    /// each set from them (04-capabilities/combat.md, A shot leaves its weapon's barrel); a weapon
    /// in two slots takes the first's.
    pub(crate) fn barrels(
        &self,
        bones: &BTreeMap<TypeName, ModelBones>,
    ) -> BTreeMap<TypeName, BarrelsData> {
        let mut barrels = BTreeMap::new();
        for (unit_type, slots) in &self.weapon_slots {
            let Some(bones) = bones.get(unit_type) else {
                continue;
            };
            let mut of_type = BarrelsData::default();
            let slotted = slots.iter().zip(&bones.barrels);
            for (weapons, of_slot) in slotted.filter(|(_, of_slot)| !of_slot.is_empty()) {
                let launches = |weapon: &&DeclaredName| {
                    let fired = self.actions.get(*weapon);
                    fired.is_some_and(|weapon| weapon.delivery.is_some())
                };
                for weapon in weapons.iter().filter(launches) {
                    of_type
                        .0
                        .entry(weapon.clone())
                        .or_insert_with(|| of_slot.clone());
                }
            }
            if !of_type.0.is_empty() {
                barrels.insert(unit_type.clone(), of_type);
            }
        }
        barrels
    }
}

impl FiredProjectile<'_> {
    /// Whether `a` and `b` are one object fired as one projectile, or both none.
    fn alike(a: Option<&Self>, b: Option<&Self>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => a.object.id == b.object.id && a.projectile == b.projectile,
            (a, b) => a.is_none() && b.is_none(),
        }
    }
}

impl ImportedWeapon {
    /// Its table under `[actions]`.
    pub(crate) fn table(&self) -> TomlTable {
        match &self.delivery {
            Some(delivery) => self.keys.table_delivering(&self.attack, delivery),
            None => self.keys.table(&self.attack),
        }
    }

    /// The attack `id` of `weapon`, by `rules`, the projectile it fires and the modifiers of its
    /// stages of continuous fire; why the import drops it, when it hits no target by the
    /// engine's rules, or holds a number the engine does not.
    fn of<'a>(
        weapon: &IniWeapon,
        rules: &'a ZeroHourRules,
        id: &DeclaredName,
    ) -> Result<ConvertedWeapon<'a>, WeaponDropCause> {
        let object = match &weapon.projectile {
            Some(_) if weapon.stream => return Err(WeaponDropCause::Stream),
            Some(name) => Some(
                rules
                    .objects
                    .object(name.as_bytes())
                    .and_then(|object| Some((object, ObjectFlight::of(object, &rules.locomotors)?)))
                    .ok_or(WeaponDropCause::NoFlight)?,
            ),
            None => None,
        };
        if weapon.detonation_objects {
            return Err(WeaponDropCause::DetonationObjects);
        }
        let splashes = weapon.radius > 0.0 || weapon.secondary_radius > 0.0;
        if splashes && object.is_none() {
            return Err(WeaponDropCause::Radius);
        }
        let scatters = weapon.scatter > 0.0 || weapon.infantry_scatter > 0.0;
        if scatters && object.is_none() {
            return Err(WeaponDropCause::Scatter);
        }
        let affects = splashes
            .then(|| Self::relations(weapon.affects).ok_or(WeaponDropCause::Affects))
            .transpose()?;
        // A weapon's anti flags pick its targets: on the ground, the units that are no air
        // target, in the air, those that are (09-zero-hour.md, A weapon's anti flags are its
        // targets).
        let targets = match (weapon.anti_ground, weapon.anti_air) {
            (true, true) => &rules.names.enemies,
            (true, false) => &rules.names.enemies_on_ground,
            (false, true) => &rules.names.enemies_in_air,
            (false, false) => return Err(WeaponDropCause::NoTarget),
        };
        if weapon.affects.suicide {
            return Err(WeaponDropCause::Suicide);
        }
        if weapon.at_self {
            return Err(WeaponDropCause::AtSelf);
        }
        let stages = Self::stages(weapon, id, &rules.names)?;
        if weapon.upgrade.changes_other_fields() {
            return Err(WeaponDropCause::UpgradeBonus);
        }
        let bonus = Self::damage_bonus(weapon.upgrade.damage())?;
        // Each shot of a clip of one empties it, so no shot waits for its delay, and its period is
        // its reload (09-zero-hour.md, A clip of one shot is a clip).
        let period = match weapon.delay {
            _ if weapon.clip_size == 1 => weapon.clip_reload_ms,
            ShotDelay::Fixed(delay) => delay,
            ShotDelay::Random => return Err(WeaponDropCause::RandomDelay),
        };
        let period_ms = match game_time::of_ms(period)? {
            0 => return Err(WeaponDropCause::NoDelay),
            ms => ms,
        };
        let damage_kind = weapon.damage_type;
        if !damage_kind.takes_life() {
            return Err(WeaponDropCause::NoLife);
        }
        if damage_kind == DamageType::Healing {
            return Err(WeaponDropCause::Healing);
        }
        let number = |value: f32| {
            let value = game_float::to_num(value).map_err(|_| WeaponDropCause::BadNumber)?;
            if value < Num::ZERO {
                return Err(WeaponDropCause::BadNumber);
            }
            Ok(value)
        };
        let ms = |ms: u64| {
            i64::try_from(ms)
                .map(Self::own)
                .map_err(|_| WeaponDropCause::BadNumber)
        };
        // A pre-attack of 0 ms waits for nothing, as `Weapon::preFireWeapon` sets none
        // (04-capabilities/combat.md, A pre-attack).
        let pre_attack = match game_time::of_ms(weapon.pre_attack_ms)? {
            0 => None,
            pre_ms => Some(PreAttackData {
                ms: ms(pre_ms)?,
                each: match weapon.pre_attack_type {
                    PrefireType::Shot => PreAttackEach::Shot,
                    PrefireType::Attack => PreAttackEach::Target,
                    PrefireType::Clip => PreAttackEach::Clip,
                },
            }),
        };
        let clip = match weapon.clip_size {
            ..0 => return Err(WeaponDropCause::BadNumber),
            0 => None,
            size => Some(ClipData {
                shots: Self::own(i64::from(size)),
                reload_ms: ms(game_time::of_ms(weapon.clip_reload_ms)?)?,
                refill: if weapon.reloads_at_home {
                    ClipRefill::Home
                } else {
                    ClipRefill::Time
                },
            }),
        };
        // Damage within a secondary radius past the primary is an outer ring; one no farther
        // than the primary deals nothing beyond it, as each unit within the primary takes the
        // primary damage alone (`dealDamageInternal`).
        let splash = match affects {
            Some(affects) => {
                let radius = number(weapon.radius)?;
                let splash = SplashData::new(radius, affects, !weapon.affects.source)
                    .ok_or(WeaponDropCause::BadNumber)?;
                if weapon.secondary_radius > weapon.radius {
                    let amount = Number::Value(Scalar::Decimal(number(weapon.secondary_damage)?));
                    let outer =
                        OuterData::new(number(weapon.secondary_radius)?, Ranked::One(amount));
                    Some(splash.with_outer(outer).ok_or(WeaponDropCause::BadNumber)?)
                } else {
                    Some(splash)
                }
            }
            None => None,
        };
        let scatter = if scatters {
            let mut tags = BTreeMap::new();
            if weapon.infantry_scatter > 0.0 {
                let infantry = rules.names.infantry.clone();
                tags.insert(infantry, number(weapon.infantry_scatter)?);
            }
            let scatter = ScatterData::new(number(weapon.scatter)?, tags);
            Some(scatter.ok_or(WeaponDropCause::BadNumber)?)
        } else {
            None
        };
        // The original takes a bound of its defaults, a turn either way, as none.
        let [low, high] = weapon.pitch;
        if !(low.is_finite() && high.is_finite()) {
            return Err(WeaponDropCause::BadNumber);
        }
        let pitch_deg = if low > -180.0 || high < 180.0 {
            let [low, high] = [low.max(-QUARTER_DEG), high.min(QUARTER_DEG)];
            let pitch =
                |value: f32| game_float::to_num(value).map_err(|_| WeaponDropCause::BadNumber);
            Some(PitchData::new(pitch(low)?, pitch(high)?).ok_or(WeaponDropCause::BadNumber)?)
        } else {
            None
        };
        // A laser's damage lands at once, and a projectile's when it ends.
        let speed = if object.is_none() && !weapon.laser && weapon.speed < IniWeapon::INSTANT {
            Some(Speed::new(number(weapon.speed)?).ok_or(WeaponDropCause::BadNumber)?)
        } else {
            None
        };
        let fired = match object {
            Some((object, flight)) => Some(FiredProjectile {
                object,
                projectile: flight.projectile(weapon, object, &rules.names)?,
            }),
            None => None,
        };
        let damage = Ranked::One(Number::Value(Scalar::Decimal(number(weapon.damage)?)));
        let meters = number(weapon.range)?;
        let range = RangeField::Range(ActionRange::meters(meters));
        let min_range = if weapon.min_range > 0.0 {
            let min = number(weapon.min_range)?;
            if min >= meters {
                return Err(WeaponDropCause::MinRange);
            }
            Some(Ranked::One(Number::Value(Scalar::Decimal(min))))
        } else {
            None
        };
        // Its attacker turns toward its target until it lies within its acceptable delta, of at
        // least 0.035 radians, as `AIAttackAimAtTargetState::update` aims; a delta of a half turn
        // needs no turn.
        let aim_deg = if weapon.aim_delta < 180.0 {
            let delta = number(weapon.aim_delta)?;
            Some(delta.max(Self::least_aim()))
        } else {
            None
        };
        let targets = targets.clone();
        Ok(ConvertedWeapon {
            weapon: Self {
                keys: ActionKeys {
                    range: Some(Ranked::One(range)),
                    min_range,
                    ..ActionKeys::new(Targeting::Unit(targets))
                },
                attack: AttackData {
                    rate: WeaponValue::Own(ms(period_ms)?),
                    rate_bonus: Some(Self::rate_bonus(&rules.names, stages.is_some())),
                    damage: WeaponValue::Own(damage),
                    damage_bonus: Some(Self::damage_bonus_stats(&rules.names, bonus.as_ref())),
                    range_bonus: Some(Self::range_bonus(&rules.names)),
                    damage_kind: damage_kind.kind(),
                    prefers: None,
                    clip,
                    splash,
                    scatter,
                    pitch_deg,
                    speed,
                    streak: stages.as_ref().map(|stages| stages.streak.clone()),
                    pre_attack,
                    aim_deg,
                },
                delivery: None,
            },
            fired,
            modifiers: stages.map_or_else(Vec::new, |stages| stages.modifiers),
            bonus,
        })
    }

    /// The least aim of a weapon, `AIAttackAimAtTargetState`'s 0.035 radians, in degrees.
    fn least_aim() -> Num {
        let radians: Num = "0.035".parse().expect("a decimal");
        radians
            .checked_mul_div(Num::int(180), Num::PI, Rounding::NearestEven)
            .expect("a small angle in degrees fits")
    }

    /// The stats a weapon's rate of fire takes: the veterancy's, the propaganda towers', and the
    /// stages' of a weapon of continuous fire, as `Weapon::computeBonus` sums each condition's
    /// bonus (04-capabilities/combat.md, A weapon's bonus is a sum of stats).
    fn rate_bonus(names: &RulesNames, staged: bool) -> BonusData {
        let stage = staged.then(|| names.rate_of_fire.clone());
        let every = [names.veterancy_rate.clone(), names.propaganda_rate.clone()];
        let stats = every.into_iter().chain(stage);
        BonusData::of(stats).expect("the veterancy's stat at least")
    }

    /// The stats a weapon's range takes: the garrison's.
    fn range_bonus(names: &RulesNames) -> BonusData {
        BonusData::of([names.garrison_range.clone()]).expect("the garrison's stat")
    }

    /// The stats a weapon's damage takes: the veterancy's, the garrison's, and its upgrade's
    /// `bonus`.
    fn damage_bonus_stats(names: &RulesNames, bonus: Option<&DamageBonus>) -> BonusData {
        let upgrade = bonus.map(|bonus| bonus.stat.clone());
        let fixed = [
            names.veterancy_damage.clone(),
            names.garrison_damage.clone(),
        ];
        let stats = fixed.into_iter().chain(upgrade);
        BonusData::of(stats).expect("the veterancy's stat at least")
    }

    /// The damage bonus of `factor`, a weapon's of `PLAYER_UPGRADE`: none for a factor of 1, which
    /// changes nothing; a bad number for a percent below 0, past a `u32` or not finite.
    fn damage_bonus(factor: f32) -> Result<Option<DamageBonus>, WeaponDropCause> {
        if factor == 1.0 {
            return Ok(None);
        }
        let percent = (factor * 100.0).round();
        if !(0.0..4_294_967_296.0).contains(&f64::from(percent)) {
            return Err(WeaponDropCause::BadNumber);
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a whole percent within a u32"
        )]
        let percent = percent as u32;
        let stat = Stat::named(&format!("upgrade_damage_{percent}")).expect("a stat's name");
        let added = game_float::to_num(factor - 1.0).map_err(|_| WeaponDropCause::BadNumber)?;
        Ok(Some(DamageBonus {
            stat,
            added: Number::Value(Scalar::Decimal(added)),
        }))
    }

    /// The streak of `weapon`'s continuous fire, of the action `id`, and the modifiers of its two
    /// stages, named after it, above `ContinuousFireOne` and `ContinuousFireTwo` shots, each of
    /// its stage's rate of fire, which adds 0 for a stage of no bonus; none for a weapon whose
    /// stages change no rate, as they change nothing. Dropped for a stage's bonus to another
    /// field, or a coast of no time, which the engine does not hold, and for shots below 0 or
    /// not growing (09-zero-hour.md, Continuous fire converts).
    fn stages(
        weapon: &IniWeapon,
        id: &DeclaredName,
        names: &RulesNames,
    ) -> Result<Option<Stages>, WeaponDropCause> {
        let fire = weapon.continuous_fire;
        if fire.changes_other_fields() {
            return Err(WeaponDropCause::ContinuousFire);
        }
        let rates = fire.rates();
        if rates == [1.0; 2] {
            return Ok(None);
        }
        let [one, two] = fire
            .shots
            .map(|shots| u32::try_from(shots).map_err(|_| WeaponDropCause::BadNumber));
        let shots = [one?, two?];
        if shots[0] >= shots[1] {
            return Err(WeaponDropCause::BadNumber);
        }
        let coast_ms = match game_time::of_ms(fire.coast_ms)? {
            0 => return Err(WeaponDropCause::ContinuousFire),
            ms => ms,
        };
        let named = |suffix| {
            ImportName::of(id.as_str().as_bytes())
                .suffixed(suffix)
                .map_err(|_| WeaponDropCause::LongName)
        };
        let mut modifiers = Vec::with_capacity(STAGES.len() + 1);
        let mut stages = Vec::with_capacity(STAGES.len());
        let staged = STAGES.into_iter().zip(&names.continuous_fire);
        for (((stage, tag), shots), rate) in staged.zip(shots).zip(rates) {
            let name = named(stage)?;
            // `WeaponBonus::appendBonuses` adds a bonus less 1, in the game's `f32`.
            let added = game_float::to_num(rate - 1.0).map_err(|_| WeaponDropCause::BadNumber)?;
            let change = Number::Value(Scalar::Decimal(added));
            modifiers.push((
                name.clone(),
                StageModifier {
                    stats: BTreeMap::from([(names.rate_of_fire.clone(), change)]),
                    tags: vec![tag.clone()],
                },
            ));
            stages.push(StageData {
                shots,
                modifier: name,
            });
        }
        let fall = named(FALL)?;
        modifiers.push((
            fall.clone(),
            StageModifier {
                stats: BTreeMap::new(),
                tags: vec![names.continuous_fire_slow.clone()],
            },
        ));
        let fall = FallData {
            modifier: fall,
            ms: game_time::frames_ms(FALL_FRAMES),
        };
        Ok(Some(Stages {
            streak: StreakData {
                coast_ms,
                stages,
                fall: Some(fall),
            },
            modifiers,
        }))
    }

    /// `value` at every rank.
    fn own(value: i64) -> Ranked<Number> {
        Ranked::One(Number::from(value))
    }

    /// The relations of `affects`, a projectile's splash, as a splash's filter names them, each
    /// player a team of its own: of `ALLIES`, `ENEMIES` and `NEUTRALS` all, enemies and neutrals,
    /// or one alone; none for another set, and for a radius that spares the airborne. `NOT_SIMILAR`
    /// changes nothing: a projectile is its blast's source, so it spares only allied projectiles of
    /// its type, which take no damage (09-zero-hour.md, `NOT_SIMILAR` of a projectile's blast
    /// converts to nothing).
    const fn relations(affects: RadiusAffects) -> Option<RelationSet> {
        if affects.not_airborne {
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
pub(crate) mod reach_in {
    use campfire_common::DeclaredName;

    use crate::zero_hour::combat_import::CombatImport;
    use crate::zero_hour::type_name::TypeName;

    impl CombatImport {
        /// Gives `unit` the weapon `weapon`, of its starting set when `starting`, else of its
        /// upgrade set alone, as the conversion of its object's sets gives it.
        pub(crate) fn give(&mut self, unit: TypeName, weapon: DeclaredName, starting: bool) {
            if starting {
                self.armed.insert(unit.clone());
            }
            self.weapons.entry(unit).or_default().push(weapon);
        }
    }
}

#[cfg(test)]
mod tests;
