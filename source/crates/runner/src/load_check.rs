use std::collections::{BTreeMap, BTreeSet};

use campfire_capabilities::{
    AbilityData, CtxEntry, DamageKind, FilterData, HeroData, Hook, Mode, ModifierData, Number,
    Param, Scalar, ScriptRole, SpellsData, Stat, UnitTypeData,
};
use campfire_content::PackagePath;
use campfire_math::Num;
use campfire_sim::Capability;

use crate::RELEASE;
use crate::error::{CtxMisuse, LoadError, LoadProblem, Place};
use crate::mode_packages::{Content, Dependent, ModePackages};
use crate::package::Package;

/// Design 08's checks at package load, over a mode and every package it depends on: data matches
/// its schema (the reads checked that), every per-rank array has an entry for each rank, every
/// script is named by data and defines only hooks of its roles, and every capability, `ctx` name,
/// modifier, param, stat, filter and damage kind a script or data names exists for the mode. And
/// every projectile is faster than the mode's move speed cap.
#[derive(Debug)]
pub(crate) struct LoadCheck<'a> {
    packages: &'a ModePackages,
    /// The tags a filter may name: those of the mode's unit types, and `hero`.
    tags: BTreeSet<&'a str>,
    /// The move speed cap, in meters a second.
    cap: Num,
}

/// The facts one package's checks share.
#[derive(Debug)]
struct PackageNames<'a> {
    package: &'a Package,
    /// The roles each script serves, as data names it.
    roles: BTreeMap<&'a PackagePath, BTreeSet<ScriptRole>>,
    /// The params each script's `ctx.p` may read.
    params: BTreeMap<&'a PackagePath, BTreeSet<&'a str>>,
    modifiers: &'a BTreeMap<String, ModifierData>,
}

/// A function whose name starts so is named like a hook, as every hook but `think` and
/// `calc_damage` is: it must be one, so a misspelled hook fails the load.
const HOOK_PREFIX: &str = "on_";

impl<'a> LoadCheck<'a> {
    pub(crate) fn run(packages: &'a ModePackages) -> Result<(), LoadError> {
        let manifest = &packages.manifest;
        let fail = |problem| LoadError {
            package: manifest.name.clone(),
            problem: Box::new(problem),
        };
        let mut tags: BTreeSet<&str> = packages
            .units
            .units
            .values()
            .flat_map(|unit_type| unit_type.tags.iter().map(String::as_str))
            .collect();
        tags.insert(UnitTypeData::HERO_TAG);
        let engines = [&packages.mode].into_iter().chain(
            packages
                .dependencies
                .iter()
                .map(|dependent| &dependent.package),
        );
        for package in engines {
            if package.engine != RELEASE {
                return Err(LoadError {
                    package: package.name.clone(),
                    problem: Box::new(LoadProblem::OtherEngine(package.engine.clone())),
                });
            }
        }
        let check = LoadCheck {
            packages,
            tags,
            cap: manifest.max_move_speed.get(),
        };
        check.mode().map_err(fail)?;
        check.spells()?;
        for dependent in &packages.dependencies {
            check.dependent(dependent).map_err(|problem| LoadError {
                package: dependent.package.name.clone(),
                problem: Box::new(problem),
            })?;
        }
        Ok(())
    }

    /// The mode package: its unit types' sections, its map, its modifiers and its scripts.
    fn mode(&self) -> Result<(), LoadProblem> {
        let packages = self.packages;
        let data = &packages.data;
        let units = &packages.units.units;
        let unit_type = |name: &str| units.contains_key(name);
        Mode::check(&packages.manifest.teams, &packages.map, unit_type)
            .map_err(LoadProblem::Mode)?;
        for (name, unit_type) in &packages.units.units {
            let at = Place::UnitType(name.clone());
            let sections = [
                (unit_type.stats.is_some(), Capability::Stats),
                (unit_type.combat.is_some(), Capability::Combat),
                (unit_type.orders.is_some(), Capability::Orders),
                (unit_type.vision.is_some(), Capability::Vision),
            ];
            for (used, capability) in sections {
                if used {
                    self.require(capability, &at)?;
                }
            }
            let attack = unit_type.combat.as_ref().and_then(|combat| combat.attack);
            self.attack_projectile(attack.and_then(|attack| attack.projectile_speed), &at)?;
        }
        if !packages.map.lanes.is_empty() {
            self.require(Capability::Navigation, &Place::Lanes)?;
        }
        for (name, field) in &data.state {
            if field.sync.is_none() {
                return Err(LoadProblem::StateSync(name.clone()));
            }
        }
        let mut names = PackageNames::new(&packages.mode, &data.modifiers);
        let mode_params: BTreeSet<&str> = data.params.keys().map(String::as_str).collect();
        names.serve(&data.script, ScriptRole::Mode, mode_params.iter().copied());
        for unit_type in packages.units.units.values() {
            if let Some(orders) = &unit_type.orders {
                names.serve(&orders.ai, ScriptRole::Ai, mode_params.iter().copied());
            }
        }
        self.modifiers(&mut names, &BTreeMap::new())?;
        self.scripts(&names)
    }

    /// No spell id is held by two spells packages, as players choose spells by id.
    fn spells(&self) -> Result<(), LoadError> {
        let mut seen = BTreeSet::new();
        for dependent in &self.packages.dependencies {
            let Content::Spells(spells) = &dependent.content else {
                continue;
            };
            if let Some(id) = spells.abilities.keys().find(|&id| !seen.insert(id)) {
                return Err(LoadError {
                    package: dependent.package.name.clone(),
                    problem: Box::new(LoadProblem::RepeatedSpell(id.clone())),
                });
            }
        }
        Ok(())
    }

    /// A hero or spells package the mode depends on.
    fn dependent(&self, dependent: &Dependent) -> Result<(), LoadProblem> {
        let package = &dependent.package;
        let (abilities, modifiers) = match &dependent.content {
            Content::Hero(hero) => {
                let at = Place::Hero(hero.name.clone());
                self.require(Capability::Combat, &at)?;
                self.require(Capability::Stats, &at)?;
                let attack = hero
                    .combat
                    .attack
                    .and_then(|attack| attack.projectile_speed);
                self.attack_projectile(attack, &at)?;
                for (at, id) in hero.slots.iter().enumerate() {
                    if !hero.abilities.contains_key(id) {
                        return Err(LoadProblem::UnknownSlot(id.clone()));
                    }
                    if hero.slots[..at].contains(id) {
                        return Err(LoadProblem::RepeatedSlot(id.clone()));
                    }
                }
                for (id, ability) in &hero.abilities {
                    let slot = hero.slots.iter().position(|slot| slot == id);
                    let slot = slot.ok_or_else(|| LoadProblem::Unslotted(id.clone()))?;
                    ranked(id, ability, HeroData::slot_ranks(slot))?;
                }
                if let Some(passive) = &hero.passive {
                    modifier_exists(&hero.modifiers, passive, &at)?;
                }
                (&hero.abilities, &hero.modifiers)
            }
            Content::Spells(spells) => {
                for (id, ability) in &spells.abilities {
                    ranked(id, ability, SpellsData::RANKS)?;
                }
                (&spells.abilities, &spells.modifiers)
            }
        };
        let mut names = PackageNames::new(package, modifiers);
        for (id, ability) in abilities {
            let at = Place::Ability(id.clone());
            self.require(Capability::Abilities, &at)?;
            if ability.projectile.is_some() {
                self.require(Capability::Projectiles, &at)?;
            }
            if ability.area.is_some() {
                self.require(Capability::Areas, &at)?;
            }
            for modifier in ability.modifiers() {
                modifier_exists(modifiers, modifier, &at)?;
            }
            for filter in ability.filters() {
                self.filter_data(filter, &at)?;
            }
            for name in ability.param_refs() {
                if !ability.params.contains_key(name) {
                    return Err(LoadProblem::UnknownParam {
                        at,
                        name: name.to_owned(),
                    });
                }
            }
            self.ability_projectile(ability, &at)?;
            if let Some(script) = &ability.script {
                names.serve(
                    script,
                    ScriptRole::Ability,
                    ability.params.keys().map(String::as_str),
                );
            }
        }
        let appliers = appliers(package, abilities);
        self.modifiers(&mut names, &appliers)?;
        self.scripts(&names)
    }

    /// The package's modifiers: the capability, the modifiers, filters and params each names, and
    /// the roles of their scripts. A modifier reads its own params, then those of the abilities
    /// that apply it, `appliers`.
    fn modifiers(
        &self,
        names: &mut PackageNames<'a>,
        appliers: &BTreeMap<&str, Vec<&'a AbilityData>>,
    ) -> Result<(), LoadProblem> {
        for (id, modifier) in names.modifiers {
            let at = Place::Modifier(id.clone());
            self.require(Capability::Stats, &at)?;
            if let Some(aura) = &modifier.aura {
                modifier_exists(names.modifiers, &aura.modifier, &at)?;
                self.filter_data(&aura.affects, &at)?;
            }
            let by = appliers.get(id.as_str()).map_or(&[][..], Vec::as_slice);
            let readable: BTreeSet<&str> = modifier
                .params
                .keys()
                .chain(by.iter().flat_map(|ability| ability.params.keys()))
                .map(String::as_str)
                .collect();
            if let Some(name) = modifier.param_refs().find(|name| !readable.contains(name)) {
                return Err(LoadProblem::UnknownParam {
                    at,
                    name: name.to_owned(),
                });
            }
            if let Some(script) = &modifier.script {
                names.serve(script, ScriptRole::Modifier, readable.iter().copied());
            }
        }
        Ok(())
    }

    /// Every script of the package: named by data, its hooks those of its roles, and every name
    /// it uses one the mode has.
    fn scripts(&self, names: &PackageNames<'_>) -> Result<(), LoadProblem> {
        let package = names.package;
        for &path in names.roles.keys() {
            if package.script(path).is_none() {
                return Err(LoadProblem::MissingScript(path.clone()));
            }
        }
        for script in &package.scripts {
            let path = &script.path;
            let Some(roles) = names.roles.get(path) else {
                return Err(LoadProblem::UnreferencedScript(path.clone()));
            };
            let at = Place::Script(path.clone());
            let facts = &script.facts;
            let misuse = |misuse| LoadProblem::CtxMisuse {
                path: path.clone(),
                misuse,
            };
            if let Some(found) = &facts.ctx_misuse {
                return Err(misuse(found.clone()));
            }
            for function in &facts.functions {
                let hook = Hook::named(&function.name);
                if hook.is_none() && function.name.starts_with(HOOK_PREFIX) {
                    return Err(LoadProblem::UnknownHook {
                        path: path.clone(),
                        function: function.name.clone(),
                    });
                }
                let Some(hook) = hook else {
                    continue;
                };
                if !roles.contains(&hook.role()) || hook.params() != function.params {
                    return Err(LoadProblem::UnknownHook {
                        path: path.clone(),
                        function: function.name.clone(),
                    });
                }
                if !function.ctx_first {
                    return Err(misuse(CtxMisuse::HookParam {
                        function: function.name.clone(),
                    }));
                }
                if let Some(capability) = hook.capability() {
                    self.require(capability, &at)?;
                }
            }
            for used in &facts.ctx_names {
                let unknown = || LoadProblem::UnknownCtx {
                    path: path.clone(),
                    name: used.name.clone(),
                };
                let entry = CtxEntry::named(&used.name).ok_or_else(unknown)?;
                let for_role = entry.role.is_none_or(|role| roles.contains(&role));
                if entry.kind != used.kind || !for_role {
                    return Err(unknown());
                }
                if let Some(capability) = entry.capability {
                    self.require(capability, &at)?;
                }
            }
            let params = &names.params[path];
            if let Some(name) = facts
                .params
                .iter()
                .find(|name| !params.contains(name.as_str()))
            {
                return Err(LoadProblem::UnknownParam {
                    at,
                    name: name.clone(),
                });
            }
            for id in &facts.modifiers {
                modifier_exists(names.modifiers, id, &at)?;
            }
            if let Some(name) = facts.stats.iter().find(|name| Stat::named(name).is_none()) {
                return Err(LoadProblem::UnknownStat {
                    at,
                    name: name.clone(),
                });
            }
            for filter in &facts.filters {
                self.filter_text(filter, &at)?;
            }
            let kinds = &facts.damage_kinds;
            if let Some(kind) = kinds.iter().find(|kind| DamageKind::parse(kind).is_none()) {
                return Err(LoadProblem::UnknownDamageKind {
                    at,
                    kind: kind.clone(),
                });
            }
        }
        Ok(())
    }

    fn require(&self, capability: Capability, at: &Place) -> Result<(), LoadProblem> {
        if self.packages.manifest.capabilities.contains(capability) {
            return Ok(());
        }
        Err(LoadProblem::Undeclared {
            capability,
            at: at.clone(),
        })
    }

    /// A script's filter: its relation is `enemies`, `allies` or `all`, and its tag, if any, one
    /// a unit type of the mode declares.
    fn filter_text(&self, filter: &str, at: &Place) -> Result<(), LoadProblem> {
        match FilterData::parse(filter) {
            Some(data) => self.filter_data(&data, at),
            None => Err(LoadProblem::UnknownFilter {
                at: at.clone(),
                filter: filter.to_owned(),
            }),
        }
    }

    /// A filter of data, whose relation its read checked: its tag, if any, is one a unit type of
    /// the mode declares.
    fn filter_data(&self, filter: &FilterData, at: &Place) -> Result<(), LoadProblem> {
        if filter
            .tag
            .as_deref()
            .is_none_or(|tag| self.tags.contains(tag))
        {
            return Ok(());
        }
        Err(LoadProblem::UnknownFilter {
            at: at.clone(),
            filter: filter.to_string(),
        })
    }

    /// An attack's projectile, of `speed` meters a second, homes, so it flies faster than the
    /// cap.
    fn attack_projectile(&self, speed: Option<Scalar>, at: &Place) -> Result<(), LoadProblem> {
        match speed.map(Scalar::to_num) {
            None => Ok(()),
            Some(Some(speed)) if speed > self.cap => Ok(()),
            Some(_) => Err(LoadProblem::ProjectileNotFaster { at: at.clone() }),
        }
    }

    /// Every speed an ability's projectile may fly at, at any rank, is faster than the cap: a
    /// script may make it home.
    fn ability_projectile(&self, ability: &AbilityData, at: &Place) -> Result<(), LoadProblem> {
        let Some(projectile) = &ability.projectile else {
            return Ok(());
        };
        for speed in projectile.speed.values() {
            let values: Vec<_> = match speed {
                Number::Value(value) => vec![*value],
                Number::Param(reference) => match ability.params.get(&reference.param) {
                    Some(Param::Ranked(ranked)) => ranked.values().to_vec(),
                    Some(Param::Scaling(scaling)) => scaling.base.values().to_vec(),
                    None => {
                        return Err(LoadProblem::UnknownParam {
                            at: at.clone(),
                            name: reference.param.clone(),
                        });
                    }
                },
            };
            for value in values {
                self.attack_projectile(Some(value), at)?;
            }
        }
        Ok(())
    }
}

impl<'a> PackageNames<'a> {
    fn new(
        package: &'a Package,
        modifiers: &'a BTreeMap<String, ModifierData>,
    ) -> PackageNames<'a> {
        PackageNames {
            package,
            roles: BTreeMap::new(),
            params: BTreeMap::new(),
            modifiers,
        }
    }

    /// Records that `script` serves `role`, and may read `params`.
    fn serve(
        &mut self,
        script: &'a PackagePath,
        role: ScriptRole,
        params: impl IntoIterator<Item = &'a str>,
    ) {
        self.roles.entry(script).or_default().insert(role);
        self.params.entry(script).or_default().extend(params);
    }
}

/// The abilities that apply each modifier: those whose data names it, and those whose script
/// adds it.
fn appliers<'a>(
    package: &'a Package,
    abilities: &'a BTreeMap<String, AbilityData>,
) -> BTreeMap<&'a str, Vec<&'a AbilityData>> {
    let mut appliers: BTreeMap<&str, Vec<&AbilityData>> = BTreeMap::new();
    for ability in abilities.values() {
        let scripted = ability
            .script
            .as_ref()
            .and_then(|path| package.script(path))
            .into_iter()
            .flat_map(|script| script.facts.modifiers.iter().map(String::as_str));
        for id in ability.modifiers().chain(scripted) {
            appliers.entry(id).or_default().push(ability);
        }
    }
    appliers
}

/// Every per-rank array of `ability` has `ranks` entries.
fn ranked(id: &str, ability: &AbilityData, ranks: u8) -> Result<(), LoadProblem> {
    if ability.check_ranks(usize::from(ranks)) {
        return Ok(());
    }
    Err(LoadProblem::RankCount {
        ability: id.to_owned(),
        ranks,
    })
}

/// `id` is one of `modifiers`.
fn modifier_exists(
    modifiers: &BTreeMap<String, ModifierData>,
    id: &str,
    at: &Place,
) -> Result<(), LoadProblem> {
    if modifiers.contains_key(id) {
        return Ok(());
    }
    Err(LoadProblem::UnknownModifier {
        at: at.to_owned(),
        id: id.to_owned(),
    })
}
