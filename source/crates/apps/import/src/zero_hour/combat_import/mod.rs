use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use campfire_client_files::ClientUnit;
use campfire_codec::TomlValue;
use campfire_combat::{ArmorShare, BarrelsData, StreakData};
use campfire_common::DeclaredName;
use campfire_projectiles::{ProjectileData, ProjectileDeliveryData};
use campfire_values::{FilterTag, Number, Stat, TagFilterData};
use serde::Serialize;

use crate::zero_hour::additions::Additions;
use crate::zero_hour::combat_import::converted_shot::{ConvertedShot, WeaponShot};
use crate::zero_hour::dropped::Losses;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::damage_type::DamageType;
use crate::zero_hour::format::ini::object_ini::fight_data::{PreferredKinds, WeaponSet};
use crate::zero_hour::format::ini::object_ini::ini_object::IniObject;
use crate::zero_hour::format::ini::weapon_ini::WeaponIni;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::{NamedType, ProjectileNames};
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::portable_structure;
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::type_name::TypeName;
use crate::zero_hour::weapon_drop_cause::WeaponDropCause;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

pub(crate) mod converted_shot;
pub(crate) mod imported_weapon;
mod weapon_gate;

use crate::zero_hour::combat_import::imported_weapon::ImportedWeapon;
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
/// and those whose body starts below its most life; the projectile types of a weapon's own, each
/// with the object whose look it takes; and the modifiers of the weapons' stages of continuous
/// fire. It gives each type its actions of the slot kind `weapon`, and the same actions by the
/// place of their slot in its sets, which its look's attack conditions test, as additions.
#[derive(Debug, Default)]
pub(crate) struct CombatImport {
    pub(crate) actions: BTreeMap<DeclaredName, ImportedWeapon>,
    /// The game's weapon each action converts, by the action.
    sources: BTreeMap<DeclaredName, String>,
    pub(crate) modifiers: BTreeMap<DeclaredName, StageModifier>,
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

    /// The combat of `rules`, of the unit types `names` made, each type's weapons given it in
    /// `additions`; the projectile types its weapons fire join `names`. An error for a type whose
    /// armor no file
    /// defines by the name of its case, which the game's debug build stops at as wrong data and
    /// its release build takes as no armor (`INI::parseArmorTemplate`), and for a projectile
    /// type's name another object took.
    pub(crate) fn of<'r>(
        rules: &'r ZeroHourRules,
        names: &mut ProjectileNames<'r>,
        additions: &mut Additions,
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
        for NamedType {
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
                        let added = additions.type_mut(type_name);
                        let slots = &mut added.weapon_slots;
                        let place = offset + slot;
                        if slots.len() <= place {
                            slots.resize_with(place + 1, Vec::new);
                        }
                        if !slots[place].contains(&id) {
                            slots[place].push(id.clone());
                        }
                        if !added.weapons.contains(&id) {
                            added.weapons.push(id);
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
    /// of their slots in `additions`, which its attack conditions test (09-zero-hour.md, A
    /// condition is the game's name for a test).
    pub(crate) fn complete_looks(
        &self,
        additions: &Additions,
        looks: &mut BTreeMap<DeclaredName, ClientUnit>,
    ) {
        for ProjectileLook { unit_type, object } in &self.projectile_looks {
            if let Some(look) = looks.get(object.declared()) {
                let look = look.clone();
                looks.insert(unit_type.declared().clone(), look);
            }
        }
        for (unit_type, added) in additions {
            if let Some(look) = looks.get_mut(unit_type.declared())
                && !added.weapon_slots.is_empty()
            {
                look.weapons.clone_from(&added.weapon_slots);
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

    /// The particle systems the projectiles of `weapons` carry, of each weapon that some action
    /// it converts to fires a projectile, its gated and carried ones among them
    /// (09-zero-hour.md, A look follows its converted action).
    pub(crate) fn exhausts<'w>(&self, weapons: &'w WeaponIni) -> Vec<&'w str> {
        let converted = self.converted_shots();
        weapons
            .weapons()
            .iter()
            .filter(|(name, _)| {
                converted.get(name.as_str()).is_some_and(|shots| {
                    shots
                        .iter()
                        .any(|shot| matches!(shot.shot, WeaponShot::Projectile(_)))
                })
            })
            .filter_map(|(_, weapon)| weapon.exhaust.as_deref())
            .collect()
    }

    /// Gives each type of `additions` the barrels of its weapons that fire a projectile: of each
    /// weapon slot of the type, the barrels its model's bones give the slot, as Zero Hour launches
    /// the slot's weapon of each set from them (04-capabilities/combat.md, A shot leaves its
    /// weapon's barrel); a weapon in two slots takes the first's.
    pub(crate) fn barrels(&self, additions: &mut Additions) {
        for (_, added) in additions.iter_mut() {
            let Some(bones) = &added.bones else {
                continue;
            };
            let mut of_type = BarrelsData::default();
            let slotted = added.weapon_slots.iter().zip(&bones.barrels);
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
                added.barrels = Some(of_type);
            }
        }
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

#[cfg(test)]
pub(crate) mod reach_in {
    use crate::zero_hour::combat_import::CombatImport;
    use crate::zero_hour::type_name::TypeName;

    impl CombatImport {
        /// Arms `unit` as it starts, as a weapon of its starting set the conversion gives it does.
        pub(crate) fn arm(&mut self, unit: TypeName) {
            self.armed.insert(unit);
        }
    }
}

#[cfg(test)]
mod tests;
