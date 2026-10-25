use std::collections::{BTreeMap, BTreeSet};

use campfire_codec::TomlValue;
use campfire_common::{DeclaredName, EnumAll};
use campfire_stats::{UpgradeData, UpgradesData};
use campfire_values::{Number, Stat};
use serde::{Deserialize, Serialize};

use crate::zero_hour::combat_import::CombatImport;
use crate::zero_hour::dropped::Losses;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::ini::object_ini::ini_upgrade_module::{
    HealthChange, IniUpgradeModule, UpgradeEffect,
};
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::TypedObject;
use crate::zero_hour::production_import::ProductionImport;
use crate::zero_hour::type_name::TypeName;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// Zero Hour's upgrades (upgrades.md, An upgrade is a modifier; upgrades.md, A unit type takes
/// modifiers by upgrades; upgrades.md, Zero Hour's upgrade flags are tags and stats): the modifier
/// of each upgrade a research gives or a module names, which gives nothing of its own; each unit
/// type's `upgrades`, an entry of each of its object's modules that converts; the modifiers of
/// their effects, each shared by the modules of one first trigger and one effect, but for a second
/// life of one type, which adds as the original's second module does; how many modules convert; and the modules and objects of sets dropped, each once for each cause, in the order
/// the types are named.
#[derive(Debug, Default)]
pub(crate) struct UpgradeImport {
    pub(crate) upgrades: BTreeSet<DeclaredName>,
    pub(crate) effects: BTreeMap<DeclaredName, EffectModifier>,
    pub(crate) types: BTreeMap<TypeName, UpgradesData>,
    pub(crate) modules: usize,
    pub(crate) dropped: Losses<UpgradeDropCause>,
}

/// The modifier of a module's effect as the mode's data writes it, `[modifiers.<name>]`: the tag
/// of the set's `PLAYER_UPGRADE` it gives, or what it adds to stats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct EffectModifier {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<DeclaredName>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    stats: BTreeMap<Stat, Number>,
}

/// Why the import drops an upgrade module, or an object's sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum UpgradeDropCause {
    /// A module of an effect other than an armor set, a weapon set, a weapon bonus or the most
    /// life, by its object's name and its class's.
    Effect,
    /// A `MaxHealthUpgrade` of a `ChangeType` other than `ADD_CURRENT_HEALTH_TOO`, which the
    /// engine's rising maximum does (04-capabilities/stats.md, A rising maximum raises the amount by
    /// as much, and a falling one keeps it).
    HealthChange,
    /// A module of no trigger, or of one no file defines.
    NoUpgrade,
    /// A `MaxHealthUpgrade` whose `AddMaxHealth` is no whole number above 0.
    BadNumber,
    /// A module of a fired projectile's type, which holds its flight alone (09-zero-hour.md, The
    /// import converts shells, splash, scatter, pitch, turrets and slow weapons).
    Projectile,
    /// The name of a trigger's or a conflict's modifier, or of the effect's, passes the bytes a
    /// declared name holds.
    LongName,
    /// An object holds a weapon set or an armor set of a condition other than none or
    /// `PLAYER_UPGRADE` alone, which it neither starts with nor takes by an upgrade, by the
    /// object's name.
    Set,
}

/// What a module's conversion reads: the rules, and the stats of the weapons' damage bonuses,
/// each with what a `WeaponBonusUpgrade` adds to it.
#[derive(Debug, Clone, Copy)]
struct ModuleSources<'a> {
    rules: &'a ZeroHourRules,
    bonuses: &'a BTreeMap<Stat, Number>,
}

impl EffectModifier {
    /// The modifier that gives the tag `tag`.
    fn tagged(tag: &DeclaredName) -> Self {
        Self {
            tags: vec![tag.clone()],
            stats: BTreeMap::new(),
        }
    }

    /// The modifier that adds `stats`, each by its number.
    const fn adding(stats: BTreeMap<Stat, Number>) -> Self {
        Self {
            tags: Vec::new(),
            stats,
        }
    }

    /// Its table under `[modifiers]`.
    pub(crate) fn table(&self) -> TomlValue {
        TomlValue::of(self)
    }
}

impl UpgradeImport {
    /// The upgrades of `rules`: the modifier of each research of `production`, and each module of
    /// the objects of `types` that converts, of the damage bonuses of `combat`'s weapons. An error
    /// for two upgrades whose names give one modifier's, and for an effect's modifier of the name
    /// of an upgrade's or of a stage's of `combat`, as one table holds them.
    pub(crate) fn of<'n, 'r: 'n>(
        rules: &ZeroHourRules,
        types: impl Iterator<Item = TypedObject<'n, 'r>>,
        production: &ProductionImport,
        combat: &CombatImport,
    ) -> Result<Self, ZeroHourError> {
        let mut named = BTreeSet::new();
        for upgrade in rules.upgrades.names() {
            let name = ImportName::latin(upgrade).declared();
            if name.is_ok_and(|name| !named.insert(name)) {
                return Err(ZeroHourError::UpgradeName(upgrade.to_owned()));
            }
        }
        let mut import = Self::default();
        let sources = ModuleSources {
            rules,
            bonuses: &combat.bonuses,
        };
        let researched = production.actions.values();
        import
            .upgrades
            .extend(researched.filter_map(|action| action.researched()).cloned());
        for TypedObject {
            name,
            object,
            imported,
        } in types
        {
            if !imported.is_projectile() && object.fight().unconverted_sets() > 0 {
                import.dropped.drop(&object.name, UpgradeDropCause::Set);
            }
            let mut entries = Vec::new();
            for module in object.upgrades() {
                let converted = if imported.is_projectile() {
                    Err(UpgradeDropCause::Projectile)
                } else {
                    import.module(sources, module, &entries)
                };
                match converted {
                    Ok(entry) => entries.push(entry),
                    Err(cause) => {
                        let dropped = format!("{} {}", object.name, module.class());
                        import.dropped.drop(&dropped, cause);
                    }
                }
            }
            import.modules += entries.len();
            if !entries.is_empty() {
                import.types.insert(name.clone(), UpgradesData(entries));
            }
        }
        let mut names = import.upgrades.iter().chain(import.effects.keys());
        let clash = names.find(|&name| {
            combat.modifiers.contains_key(name)
                || (import.upgrades.contains(name) && import.effects.contains_key(name))
        });
        if let Some(clash) = clash {
            return Err(ZeroHourError::UpgradeName(clash.as_str().to_owned()));
        }
        Ok(import)
    }

    /// The entry of `module`, of a type of the entries `held` before it, its trigger's and its
    /// conflicts' modifiers kept and its effect's made; why the import drops it. A conflict no file
    /// defines is no upgrade any player or object holds, so it keeps nothing from triggering, and
    /// goes. A life its type holds an entry of already takes the next of `_2`, `_3` and so on
    /// after its name, as each `MaxHealthUpgrade` adds its own, where a tag or a weapon's bonus
    /// is a flag the original sets once however many modules set it.
    fn module(
        &mut self,
        sources: ModuleSources<'_>,
        module: &IniUpgradeModule,
        held: &[UpgradeData],
    ) -> Result<UpgradeData, UpgradeDropCause> {
        let ModuleSources { rules, bonuses } = sources;
        let modifier = |name: &str| {
            ImportName::latin(name)
                .declared()
                .map_err(|_| UpgradeDropCause::LongName)
        };
        let Some(first) = module.triggers.first() else {
            return Err(UpgradeDropCause::NoUpgrade);
        };
        if module
            .triggers
            .iter()
            .any(|name| rules.upgrades.get(name).is_none())
        {
            return Err(UpgradeDropCause::NoUpgrade);
        }
        let triggers = module
            .triggers
            .iter()
            .map(|name| modifier(name))
            .collect::<Result<Vec<_>, _>>()?;
        let conflicts = module.conflicts.iter();
        let conflicts = conflicts
            .filter(|name| rules.upgrades.get(name).is_some())
            .map(|name| modifier(name))
            .collect::<Result<Vec<_>, _>>()?;
        let names = &rules.names;
        let (suffix, effect) = match module.effect {
            UpgradeEffect::Armor => (
                "armor".to_owned(),
                EffectModifier::tagged(&names.armor_upgrade),
            ),
            UpgradeEffect::WeaponSet => (
                "weapons".to_owned(),
                EffectModifier::tagged(&names.weapon_upgrade),
            ),
            UpgradeEffect::WeaponBonus => {
                ("bonus".to_owned(), EffectModifier::adding(bonuses.clone()))
            }
            UpgradeEffect::MaxHealth { add, change } => {
                if change != HealthChange::AddCurrentToo {
                    return Err(UpgradeDropCause::HealthChange);
                }
                if !(add > 0.0 && add.fract() == 0.0 && add < 1e9) {
                    return Err(UpgradeDropCause::BadNumber);
                }
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a whole number above 0 and below 10⁹"
                )]
                let add = add as u32;
                let life = (names.life_stat.clone(), Number::from(i64::from(add)));
                (
                    format!("life_{add}"),
                    EffectModifier::adding(BTreeMap::from([life])),
                )
            }
            UpgradeEffect::Other(_) => return Err(UpgradeDropCause::Effect),
        };
        let adds = matches!(module.effect, UpgradeEffect::MaxHealth { .. });
        let mut name = ImportName::latin(first)
            .suffixed(&suffix)
            .map_err(|_| UpgradeDropCause::LongName)?;
        let base = name.clone();
        let mut next = 2_u32;
        while adds && held.iter().any(|entry| entry.modifier == name) {
            name = ImportName::of(base.as_str().as_bytes())
                .suffixed(&next.to_string())
                .map_err(|_| UpgradeDropCause::LongName)?;
            next += 1;
        }
        self.upgrades.extend(triggers.iter().cloned());
        self.upgrades.extend(conflicts.iter().cloned());
        self.effects.insert(name.clone(), effect);
        Ok(UpgradeData {
            triggers,
            all: module.all,
            conflicts,
            when: None,
            modifier: name,
        })
    }
}

#[cfg(test)]
mod tests;
