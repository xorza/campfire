use std::collections::{BTreeMap, BTreeSet};

use campfire_common::DeclaredName;
use campfire_stats::{HeldSources, UpgradeData};
use campfire_values::{FilterData, Number, Stat};
use serde::Serialize;

use crate::zero_hour::error::{NameError, ZeroHourError};
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::object_ini::ini_propaganda::IniPropaganda;
use crate::zero_hour::format::ini::object_ini::ini_upgrade_module::IniUpgradeModule;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::TypedObject;
use crate::zero_hour::portable_structure;
use crate::zero_hour::type_name::TypeName;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// Zero Hour's propaganda towers as auras (riders-and-upgrades.md, A propaganda tower is an
/// aura): the entries of the upgrades of each type of a `PropagandaTowerBehavior` of its own or
/// whose upgrade makes a portable tower inside it, the upgrades their entries name, and the
/// modifiers of the auras and of their heals.
#[derive(Debug, Default)]
pub(crate) struct PropagandaImport {
    pub(crate) upgrades: BTreeMap<TypeName, Vec<UpgradeData>>,
    pub(crate) named: BTreeSet<DeclaredName>,
    pub(crate) modifiers: BTreeMap<DeclaredName, PropagandaModifier>,
}

/// A tower's modifier as the mode's data writes it, `[modifiers.<name>]`: an aura's, which holds
/// its heal on the allied units within its radius, or a heal's, of what it adds to the share of
/// life regained and to the rate of fire, held from one tower at a time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PropagandaModifier {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    stats: BTreeMap<Stat, Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    held_sources: Option<HeldSources>,
    #[serde(skip_serializing_if = "Option::is_none")]
    aura: Option<PropagandaAura>,
}

/// An aura as the mode's data writes it: its radius, the units it takes, and the modifier it
/// holds on them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PropagandaAura {
    radius: Number,
    affects: FilterData,
    modifier: DeclaredName,
}

/// The names of a tower's modifiers: its aura's and its heal's, and those of its upgraded heal.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TowerNames {
    aura: DeclaredName,
    heal: DeclaredName,
    upgraded: DeclaredName,
    upgraded_heal: DeclaredName,
}

impl PropagandaImport {
    /// The towers of `types`, the objects of `rules`: a type's own, as the Speaker Tower's, an
    /// upgrade of no trigger, and its upgraded heal one of its `UpgradeRequired`, both while the
    /// unit is no site, as `PropagandaTowerBehavior::update` skips a tower under construction;
    /// a portable tower's, as the Overlord's, an upgrade of the triggers of the module that makes
    /// it, and its upgraded heal one of those and its `UpgradeRequired`, both of the module's
    /// conflicts. An error for a name past the bytes a declared name holds, or a number of no
    /// `Num`.
    pub(crate) fn of<'n, 'r>(
        rules: &'r ZeroHourRules,
        types: impl IntoIterator<Item = TypedObject<'n, 'r>>,
    ) -> Result<Self, ZeroHourError> {
        let mut import = Self::default();
        for TypedObject { name, object, .. } in types {
            if let Some(tower) = object.propaganda() {
                let names = TowerNames::of(name.declared())?;
                let built = |triggers, modifier: &DeclaredName| UpgradeData {
                    triggers,
                    all: true,
                    conflicts: Vec::new(),
                    when: Some(rules.names.built.clone()),
                    modifier: modifier.clone(),
                };
                let entries = import.upgrades.entry(name.clone()).or_default();
                entries.push(built(Vec::new(), &names.aura));
                if let Some(required) = &tower.upgrade {
                    let required = ImportName::latin(required).declared()?;
                    entries.push(built(vec![required.clone()], &names.upgraded));
                    import.named.insert(required);
                }
                import.tower(tower, &names, rules)?;
            }
            for module in object.upgrades() {
                let Some(structure) = portable_structure::structure(rules, module) else {
                    continue;
                };
                let Some(tower) = structure.propaganda() else {
                    continue;
                };
                let structure = ImportName::latin(&structure.name).declared()?;
                let names = TowerNames::of(&structure)?;
                let entries = import.carried(module, tower, &names)?;
                import
                    .upgrades
                    .entry(name.clone())
                    .or_default()
                    .extend(entries);
                import.tower(tower, &names, rules)?;
            }
        }
        Ok(import)
    }

    /// The entries of the upgrades of a carrier whose module `module` makes the portable tower
    /// `tower`, of `names`: its aura by the module's triggers, and its upgraded heal by those and
    /// its `UpgradeRequired`, both of the module's conflicts.
    fn carried(
        &mut self,
        module: &IniUpgradeModule,
        tower: &IniPropaganda,
        names: &TowerNames,
    ) -> Result<Vec<UpgradeData>, NameError> {
        let named = |names: &[String]| -> Result<Vec<DeclaredName>, NameError> {
            names
                .iter()
                .map(|name| ImportName::latin(name).declared())
                .collect()
        };
        let triggers = named(&module.triggers)?;
        let conflicts = named(&module.conflicts)?;
        self.named
            .extend(triggers.iter().chain(&conflicts).cloned());
        let mut entries = vec![UpgradeData {
            triggers: triggers.clone(),
            all: module.all,
            conflicts: conflicts.clone(),
            when: None,
            modifier: names.aura.clone(),
        }];
        if let Some(required) = &tower.upgrade {
            let required = ImportName::latin(required).declared()?;
            self.named.insert(required.clone());
            entries.push(UpgradeData {
                triggers: triggers.into_iter().chain([required]).collect(),
                all: true,
                conflicts,
                when: None,
                modifier: names.upgraded.clone(),
            });
        }
        Ok(entries)
    }

    /// The modifiers of `tower`, of `names`: its aura and its upgraded aura, each of its radius on
    /// the allied units but structures, and their heals, the upgraded one of what its heal adds
    /// past the base, each with the rate of fire of `ENTHUSIASTIC` and of `SUBLIMINAL` less 1, as
    /// `WeaponBonus::appendBonuses` adds a bonus.
    fn tower(
        &mut self,
        tower: &IniPropaganda,
        names: &TowerNames,
        rules: &ZeroHourRules,
    ) -> Result<(), ZeroHourError> {
        let (game, rules) = (&rules.game_data, &rules.names);
        let num = |value: f32| {
            game_float::to_num(value)
                .map(Number::from)
                .map_err(|_| ZeroHourError::Propaganda)
        };
        let radius = num(tower.radius)?;
        let aura = |modifier: &DeclaredName| PropagandaModifier {
            stats: BTreeMap::new(),
            held_sources: None,
            aura: Some(PropagandaAura {
                radius: radius.clone(),
                affects: rules.allied_units.clone(),
                modifier: modifier.clone(),
            }),
        };
        let heal = |share: f32, rate: f32| -> Result<PropagandaModifier, ZeroHourError> {
            Ok(PropagandaModifier {
                stats: BTreeMap::from([
                    (rules.life_regen_share.clone(), num(share)?),
                    (rules.propaganda_rate.clone(), num(rate - 1.0)?),
                ]),
                held_sources: Some(HeldSources::One),
                aura: None,
            })
        };
        let modifiers = [
            (names.aura.clone(), aura(&names.heal)),
            (
                names.heal.clone(),
                heal(tower.heal, game.enthusiastic_rate)?,
            ),
            (names.upgraded.clone(), aura(&names.upgraded_heal)),
            (
                names.upgraded_heal.clone(),
                heal(tower.upgraded_heal - tower.heal, game.subliminal_rate)?,
            ),
        ];
        self.modifiers.extend(modifiers);
        Ok(())
    }
}

impl TowerNames {
    /// The names of the modifiers of the tower `tower`, a type's or a portable structure's.
    fn of(tower: &DeclaredName) -> Result<Self, NameError> {
        let name = |suffix: &str| ImportName::of(tower.as_str().as_bytes()).suffixed(suffix);
        Ok(Self {
            aura: name("propaganda")?,
            heal: name("propaganda_heal")?,
            upgraded: name("propaganda_upgraded")?,
            upgraded_heal: name("propaganda_upgraded_heal")?,
        })
    }
}

#[cfg(test)]
mod tests;
