use std::collections::{BTreeMap, BTreeSet};
use std::iter;

use campfire_capabilities::{
    AbilityData, ApiOwner, CollisionData, DeclaredName, EngineStat, FilterData, Hook, MemberKind,
    Mode, ModifierData, Navigation, Number, Param, PoolId, Pools, Scalar, ScriptApi, ScriptRole,
    Stat, UnitTypeData,
};
use campfire_content::PackagePath;
use campfire_math::Num;
use campfire_sim::Capability;

use crate::RELEASE_VERSION;
use crate::error::{CtxMisuse, LoadError, LoadProblem, Place};
use crate::files::avatar_data::AvatarData;
use crate::files::loadout_data::LoadoutData;
use crate::mode_packages::{Content, Dependent, ModePackages};
use crate::package::Package;
use crate::script_facts::ScriptFacts;

/// Design 08's checks at package load, over a mode and every package it depends on: data matches
/// its schema (the reads checked that), every per-rank array has an entry for each rank, every
/// script is named by data and defines only hooks of its roles, and every capability, `ctx` name,
/// modifier, param, stat, filter and damage kind a script or data names exists for the mode. And
/// every projectile is faster than the mode's move speed cap.
#[derive(Debug)]
pub(crate) struct LoadCheck<'a> {
    packages: &'a ModePackages,
    /// The tags a filter may name: every tag the mode's packages name.
    tags: BTreeSet<&'a str>,
    /// The move speed cap, in meters a second.
    cap: Num,
    /// The script API of the release, which every name a script uses must be of.
    api: ScriptApi,
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

impl<'a> LoadCheck<'a> {
    pub(crate) fn run(packages: &'a ModePackages) -> Result<(), LoadError> {
        let manifest = &packages.manifest;
        let fail = |problem| LoadError {
            package: manifest.header.name.clone(),
            problem: Box::new(problem),
        };
        let tags = packages.tag_names();
        if tags.len() > UnitTypeData::TAG_LIMIT {
            return Err(fail(LoadProblem::TooManyTags));
        }
        let avatars = packages.dependencies.iter().filter_map(|dependent| {
            matches!(dependent.content, Content::Avatar(_)).then_some(&dependent.package.name)
        });
        let mut unit_types = packages.units.units.len();
        for avatar in avatars {
            if packages.units.units.contains_key(avatar) {
                return Err(fail(LoadProblem::RepeatedUnitType(avatar.clone())));
            }
            unit_types += 1;
        }
        if unit_types > UnitTypeData::TYPE_LIMIT {
            return Err(fail(LoadProblem::TooManyUnitTypes));
        }
        let engines = [&packages.mode].into_iter().chain(
            packages
                .dependencies
                .iter()
                .map(|dependent| &dependent.package),
        );
        for package in engines {
            if package.engine != RELEASE_VERSION {
                return Err(LoadError {
                    package: package.name.clone(),
                    problem: Box::new(LoadProblem::OtherEngine(package.engine)),
                });
            }
        }
        let check = LoadCheck {
            packages,
            tags,
            cap: manifest.max_move_speed.get(),
            api: ScriptApi::release(),
        };
        check.mode().map_err(fail)?;
        check.loadout()?;
        for dependent in &packages.dependencies {
            check.dependent(dependent).map_err(|problem| LoadError {
                package: dependent.package.name.clone(),
                problem: Box::new(problem),
            })?;
        }
        packages
            .stat_graph()
            .order()
            .map_err(|stats| fail(LoadProblem::StatLoop(stats)))?;
        Ok(())
    }

    /// The map can be walked by every unit that walks, among the mode's unit types and its
    /// avatars, as `Navigation::check_map` sets: the widest of each layer stands on every
    /// marker's point and waypoint, and reaches every waypoint from the one before, among the
    /// map's placed units that cannot walk.
    fn map_walkable(&self) -> Result<(), LoadProblem> {
        let packages = self.packages;
        let walkers = packages.walkers();
        let move_speed = Stat::Engine(EngineStat::MoveSpeed);
        let body_of = |unit_type: &str| {
            let unit_type = packages.units.units.get(unit_type)?;
            let walks = unit_type
                .stats
                .as_ref()
                .is_some_and(|stats| stats.declares(&move_speed));
            let body = packages.data.navigation.body(unit_type.collision.as_ref());
            body.filter(|_| !walks)
        };
        Navigation::check_map(&packages.map, &walkers, body_of).map_err(LoadProblem::Map)
    }

    /// The mode package: its unit types' sections, its map, its modifiers and its scripts.
    fn mode(&self) -> Result<(), LoadProblem> {
        let packages = self.packages;
        let data = &packages.data;
        let units = &packages.units.units;
        let unit_type = |name: &str| units.contains_key(name);
        Mode::check(
            &packages.manifest.teams,
            &data.relations,
            &packages.map,
            unit_type,
        )
        .map_err(LoadProblem::Mode)?;
        for list in [
            &data.combat.damage_kinds,
            &data.resources,
            &data.navigation.layers,
        ] {
            let mut seen = BTreeSet::new();
            if let Some(name) = list.iter().find(|&name| !seen.insert(name)) {
                return Err(LoadProblem::RepeatedName(name.clone()));
            }
        }
        self.damage_kinds()?;
        self.pools()?;
        self.layers()?;
        if data.combat.stats().next().is_some() || data.combat.life.is_some() {
            self.require(Capability::Combat, &Place::Combat)?;
            self.stats_declared(data.combat.stats(), &Place::Combat)?;
        }
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
            if let Some(stats) = &unit_type.stats {
                self.stats_declared(stats.0.keys(), &at)?;
            }
            self.unit_pools(&unit_type.pools, unit_type.combat.is_some(), &at)?;
            self.collision_layer(unit_type.collision.as_ref(), &at)?;
            let attack = unit_type.combat.as_ref().and_then(|combat| combat.attack);
            self.attack_projectile(attack.and_then(|attack| attack.projectile_speed), &at)?;
        }
        if !packages.map.paths.is_empty() {
            self.require(Capability::Navigation, &Place::Paths)?;
        }
        self.map_walkable()?;
        if packages.manifest.capabilities.contains(Capability::Vision)
            && packages.map.grid.is_none()
        {
            return Err(LoadProblem::NoGrid);
        }
        if packages
            .manifest
            .capabilities
            .contains(Capability::Navigation)
            && packages.map.navigation.is_none()
        {
            return Err(LoadProblem::NoPathingGrid);
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

    /// No entry id is held by two loadout packages, as players choose loadout entries by id.
    fn loadout(&self) -> Result<(), LoadError> {
        let mut seen = BTreeSet::new();
        for dependent in &self.packages.dependencies {
            let Content::Loadout(loadout) = &dependent.content else {
                continue;
            };
            if let Some(id) = loadout.abilities.keys().find(|&id| !seen.insert(id)) {
                return Err(LoadError {
                    package: dependent.package.name.clone(),
                    problem: Box::new(LoadProblem::RepeatedLoadout(id.clone())),
                });
            }
        }
        Ok(())
    }

    /// An avatar or loadout package the mode depends on.
    fn dependent(&self, dependent: &Dependent) -> Result<(), LoadProblem> {
        let package = &dependent.package;
        let (abilities, modifiers) = match &dependent.content {
            Content::Avatar(avatar) => {
                let at = Place::Avatar(avatar.name.clone());
                self.require(Capability::Combat, &at)?;
                self.require(Capability::Stats, &at)?;
                if avatar.vision.is_some() {
                    self.require(Capability::Vision, &at)?;
                }
                self.stats_declared(avatar.stats.0.keys(), &at)?;
                self.unit_pools(&avatar.pools, true, &at)?;
                self.collision_layer(avatar.collision.as_ref(), &at)?;
                let attack = avatar
                    .combat
                    .attack
                    .and_then(|attack| attack.projectile_speed);
                self.attack_projectile(attack, &at)?;
                for (at, id) in avatar.slots.iter().enumerate() {
                    if !avatar.abilities.contains_key(id) {
                        return Err(LoadProblem::UnknownSlot(id.clone()));
                    }
                    if avatar.slots[..at].contains(id) {
                        return Err(LoadProblem::RepeatedSlot(id.clone()));
                    }
                }
                for (id, ability) in &avatar.abilities {
                    let slot = avatar.slots.iter().position(|slot| slot == id);
                    let slot = slot.ok_or_else(|| LoadProblem::Unslotted(id.clone()))?;
                    self.ranked(id, ability, AvatarData::slot_ranks(slot))?;
                }
                if let Some(passive) = &avatar.passive {
                    modifier_exists(&avatar.modifiers, passive, &at)?;
                }
                (&avatar.abilities, &avatar.modifiers)
            }
            Content::Loadout(loadout) => {
                for (id, ability) in &loadout.abilities {
                    self.ranked(id, ability, LoadoutData::RANKS)?;
                }
                (&loadout.abilities, &loadout.modifiers)
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
            self.stats_declared(ability.params.values().flat_map(Param::stats), &at)?;
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
                    ScriptRole::Action,
                    ability.params.keys().map(String::as_str),
                );
            }
        }
        let appliers = package.appliers(abilities);
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
            let scaled = modifier.params.values().flat_map(Param::stats);
            self.stats_declared(modifier.stats.keys().chain(scaled), &at)?;
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
                if hook.is_none()
                    && Hook::PREFIXES
                        .iter()
                        .any(|prefix| function.name.starts_with(prefix))
                {
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
                let member = self.api.member(ApiOwner::Ctx, &used.name).filter(|member| {
                    member.kind == used.kind
                        && roles.iter().any(|&role| member.roles.contains(role))
                });
                let Some(member) = member else {
                    return Err(LoadProblem::UnknownCtx {
                        path: path.clone(),
                        name: used.name.clone(),
                    });
                };
                if let Some(capability) = member.capability {
                    self.require(capability, &at)?;
                }
            }
            for used in &facts.members {
                if !self.member_known(facts, &used.name, used.kind) {
                    return Err(LoadProblem::UnknownMember {
                        path: path.clone(),
                        name: used.name.clone(),
                    });
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
            for filter in &facts.filters {
                self.filter_text(filter, &at)?;
            }
            self.script_vocabulary(facts, at)?;
        }
        Ok(())
    }

    /// Whether a field or method `name` that a script with `facts` reads on a value is one some
    /// handle has, one the engine has of its own, a key of the script's object maps, or one of
    /// its functions, called as a method.
    fn member_known(&self, facts: &ScriptFacts, name: &str, kind: MemberKind) -> bool {
        let handle = self.api.members().iter().any(|member| {
            member.owner != ApiOwner::Ctx && member.name == name && member.kind == kind
        });
        handle
            || match kind {
                MemberKind::Field => {
                    self.api.builtin(&format!("get${name}"))
                        || facts.map_keys.iter().any(|key| key == name)
                }
                _ => {
                    self.api.builtin(name)
                        || facts.functions.iter().any(|function| function.name == name)
                }
            }
    }

    /// Every stat, pool and damage kind a script at `at` with `facts` names is one the engine
    /// reads or the mode declares.
    fn script_vocabulary(&self, facts: &ScriptFacts, at: Place) -> Result<(), LoadProblem> {
        for name in &facts.stats {
            let stat = Stat::named(name).ok_or_else(|| LoadProblem::UnknownStat {
                at: at.clone(),
                name: name.clone(),
            })?;
            self.stats_declared([&stat], &at)?;
        }
        let data = &self.packages.data;
        let tag = |name: &String| {
            let markers = &self.packages.map.markers;
            markers.iter().any(|marker| marker.tags.contains(name))
        };
        if let Some(name) = facts.markers.iter().find(|name| !tag(name)) {
            return Err(LoadProblem::UnknownMarkerTag {
                at,
                tag: name.clone(),
            });
        }
        let pool = |name: &String| data.pools.keys().any(|pool| pool.as_str() == name);
        if let Some(name) = facts.pools.iter().find(|name| !pool(name)) {
            return Err(LoadProblem::UnknownPool {
                at,
                name: name.clone(),
            });
        }
        let declared = |kind: &String| {
            data.combat
                .damage_kinds
                .iter()
                .any(|name| name.as_str() == kind)
        };
        if let Some(kind) = facts.damage_kinds.iter().find(|kind| !declared(kind)) {
            return Err(LoadProblem::UnknownDamageKind {
                at,
                kind: kind.clone(),
            });
        }
        Ok(())
    }

    /// The mode's damage kinds and `attack_kind`: with `combat`, at least one kind and an attack
    /// kind among them; never more kinds than a byte tells apart.
    fn damage_kinds(&self) -> Result<(), LoadProblem> {
        let data = &self.packages.data;
        let kinds = &data.combat.damage_kinds;
        if kinds.len() > usize::from(u8::MAX) + 1 {
            return Err(LoadProblem::TooManyDamageKinds);
        }
        if let Some(kind) = &data.attack_kind {
            self.require(Capability::Combat, &Place::AttackKind)?;
            if !kinds.contains(kind) {
                return Err(LoadProblem::UnknownDamageKind {
                    at: Place::AttackKind,
                    kind: kind.as_str().to_owned(),
                });
            }
        }
        if !self
            .packages
            .manifest
            .capabilities
            .contains(Capability::Combat)
        {
            return Ok(());
        }
        if kinds.is_empty() {
            return Err(LoadProblem::NoDamageKinds);
        }
        if data.attack_kind.is_none() {
            return Err(LoadProblem::NoAttackKind);
        }
        Ok(())
    }

    /// The mode's layers are navigation's.
    fn layers(&self) -> Result<(), LoadProblem> {
        if self.packages.data.navigation.layers.is_empty() {
            return Ok(());
        }
        self.require(Capability::Navigation, &Place::Navigation)
    }

    /// The layer a `collision` section at `at` names, if any, is one the mode declares.
    fn collision_layer(
        &self,
        collision: Option<&CollisionData>,
        at: &Place,
    ) -> Result<(), LoadProblem> {
        let navigation = &self.packages.data.navigation;
        match collision.and_then(|collision| collision.layer.as_ref()) {
            Some(layer) if navigation.layer(layer).is_none() => Err(LoadProblem::UnknownLayer {
                at: at.clone(),
                layer: layer.clone(),
            }),
            _ => Ok(()),
        }
    }

    /// The mode's pools: at most `Pools::LIMIT`, none named as a player resource, each with
    /// stats the mode declares; and with `combat`, a life pool among them.
    fn pools(&self) -> Result<(), LoadProblem> {
        let data = &self.packages.data;
        if data.pools.len() > Pools::LIMIT {
            return Err(LoadProblem::TooManyPools);
        }
        if let Some(name) = data
            .resources
            .iter()
            .find(|name| data.pools.contains_key(name))
        {
            return Err(LoadProblem::RepeatedName(name.clone()));
        }
        for (name, pool) in &data.pools {
            let at = Place::Pool(name.clone());
            self.require(Capability::Stats, &at)?;
            self.stats_declared(iter::once(&pool.max).chain(&pool.regen), &at)?;
        }
        if let Some(life) = &data.combat.life
            && !data.pools.contains_key(life)
        {
            return Err(LoadProblem::UnknownPool {
                at: Place::Combat,
                name: life.to_string(),
            });
        }
        let combat = self
            .packages
            .manifest
            .capabilities
            .contains(Capability::Combat);
        if combat && data.combat.life.is_none() {
            return Err(LoadProblem::NoLifePool);
        }
        Ok(())
    }

    /// The pools a unit type at `at` lists: each declared, none twice, and the life pool among
    /// them when it has `combat`.
    fn unit_pools(
        &self,
        pools: &[DeclaredName],
        combat: bool,
        at: &Place,
    ) -> Result<(), LoadProblem> {
        let data = &self.packages.data;
        if !pools.is_empty() {
            self.require(Capability::Stats, at)?;
        }
        for (place, name) in pools.iter().enumerate() {
            if !data.pools.contains_key(name) {
                return Err(LoadProblem::UnknownPool {
                    at: at.clone(),
                    name: name.to_string(),
                });
            }
            if pools[..place].contains(name) {
                return Err(LoadProblem::RepeatedPool {
                    at: at.clone(),
                    name: name.clone(),
                });
            }
        }
        let life = data.combat.life.as_ref();
        if combat && !life.is_some_and(|life| pools.contains(life)) {
            return Err(LoadProblem::LifePoolMissing(at.clone()));
        }
        Ok(())
    }

    /// Every per-rank array of `ability` has `ranks` entries, each pool its cost names is one
    /// the mode declares, and each capability field holds at every rank.
    fn ranked(&self, id: &str, ability: &AbilityData, ranks: u8) -> Result<(), LoadProblem> {
        let pools = &self.packages.data.pools;
        if let Some(name) = ability.cost_pools().find(|name| !pools.contains_key(name)) {
            return Err(LoadProblem::UnknownPool {
                at: Place::Ability(id.to_owned()),
                name: name.to_string(),
            });
        }
        if !ability.check_ranks(usize::from(ranks)) {
            return Err(LoadProblem::RankCount {
                ability: id.to_owned(),
                ranks,
            });
        }
        for rank in 1..=ranks {
            ability
                .fields_at(rank, |name| PoolId::of(pools, name))
                .map_err(|field| LoadProblem::AbilityField {
                    ability: id.to_owned(),
                    field,
                })?;
        }
        Ok(())
    }

    /// Every stat in `stats` is one the engine reads or the mode declares.
    fn stats_declared<'s>(
        &self,
        stats: impl IntoIterator<Item = &'s Stat>,
        at: &Place,
    ) -> Result<(), LoadProblem> {
        let declared = &self.packages.data.stats;
        let unknown = stats.into_iter().find(|stat| !declared.contains_key(stat));
        match unknown {
            Some(name) => Err(LoadProblem::UnknownStat {
                at: at.clone(),
                name: name.to_string(),
            }),
            None => Ok(()),
        }
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

    /// A filter of data, whose relation its read checked: each of its tags is one the mode's
    /// packages name.
    fn filter_data(&self, filter: &FilterData, at: &Place) -> Result<(), LoadProblem> {
        if filter
            .tags
            .iter()
            .all(|tag| self.tags.contains(tag.name.as_str()))
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
