use std::collections::{BTreeMap, BTreeSet};
use std::iter;

use campfire_capabilities::{
    ActionData, ActionKind, ActionSlots, ApiOwner, CollisionData, DeclaredName, EngineStat,
    FilterData, Hook, MemberKind, Mode, ModifierData, Navigation, Number, Offers, Param, Pools,
    Range, RangeField, ResourceId, Scalar, ScriptApi, ScriptRole, Stat, Targeting, UnitTypeData,
};
use campfire_content::PackagePath;
use campfire_math::Num;
use campfire_sim::Capability;

use crate::RELEASE_VERSION;
use crate::error::{ChoiceProblem, CtxMisuse, LoadError, LoadProblem, NameKind, Place};
use crate::files::units_data::UnitTypeFile;
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
        self.pools_and_resources()?;
        self.layers()?;
        if data.combat.stats().next().is_some() || data.combat.life.is_some() {
            self.require(Capability::Combat, &Place::Combat)?;
            self.stats_declared(data.combat.stats(), &Place::Combat)?;
        }
        self.slot_kinds()?;
        self.choices()?;
        for (name, unit_type) in &packages.units.units {
            let at = Place::UnitType(name.clone());
            self.unit_type(unit_type, &at, &data.actions, &data.modifiers)?;
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
        let ranks = self.slotted_ranks(packages.units.units.values())?;
        let ranks = |id: &str| ranks.get(id).copied().unwrap_or(1);
        self.actions(&data.actions, ranks, &mut names)?;
        let appliers = packages.mode.appliers(&data.actions);
        self.modifiers(&mut names, &appliers)?;
        self.scripts(&names)
    }

    /// No entry id is held by two loadout packages, as players choose loadout entries by id.
    fn loadout(&self) -> Result<(), LoadError> {
        let mut seen = BTreeSet::new();
        for dependent in &self.packages.dependencies {
            let Content::Loadout(loadout) = &dependent.content else {
                continue;
            };
            if let Some(id) = loadout.actions.keys().find(|&id| !seen.insert(id)) {
                return Err(LoadError {
                    package: dependent.package.name.clone(),
                    problem: Box::new(LoadProblem::RepeatedLoadout(id.clone())),
                });
            }
        }
        Ok(())
    }

    /// An avatar or loadout package the mode depends on: an avatar's unit type, with every
    /// action of the package in its slots, and each action with the ranks of its kind; a
    /// loadout's actions, each with the ranks of the slot kind its choice fills.
    fn dependent(&self, dependent: &Dependent) -> Result<(), LoadProblem> {
        let package = &dependent.package;
        let (actions, modifiers, slotted) = match &dependent.content {
            Content::Avatar(avatar) => {
                if avatar.unit.orders.is_some() {
                    return Err(LoadProblem::AvatarOrders);
                }
                let at = Place::Avatar(avatar.name.clone());
                self.unit_type(&avatar.unit, &at, &avatar.actions, &avatar.modifiers)?;
                let ranks = self.slotted_ranks([&avatar.unit])?;
                let unslotted = avatar
                    .actions
                    .keys()
                    .find(|id| !ranks.contains_key(id.as_str()));
                if let Some(id) = unslotted {
                    return Err(LoadProblem::Unslotted(id.clone()));
                }
                (&avatar.actions, &avatar.modifiers, Some(ranks))
            }
            Content::Loadout(loadout) => (&loadout.actions, &loadout.modifiers, None),
        };
        let loadout_ranks = self.packages.data.loadout_ranks();
        let ranks = |id: &str| slotted.as_ref().map_or(loadout_ranks, |ranks| ranks[id]);
        let mut names = PackageNames::new(package, modifiers);
        self.actions(actions, ranks, &mut names)?;
        let appliers = package.appliers(actions);
        self.modifiers(&mut names, &appliers)?;
        self.scripts(&names)
    }

    /// The ranks `types` give each action they place, as `ModePackages::slotted_ranks` reads
    /// them; an action placed in kinds of other ranks fails.
    fn slotted_ranks<'u>(
        &self,
        types: impl IntoIterator<Item = &'u UnitTypeFile>,
    ) -> Result<BTreeMap<&'u str, u8>, LoadProblem> {
        self.packages
            .slotted_ranks(types)
            .map_err(|id| LoadProblem::ActionRanks(id.to_owned()))
    }

    /// A package's `actions`, each with the ranks `ranks` gives it, and the roles of their
    /// scripts in `names`: the capabilities each uses, and the modifiers, filters, stats and
    /// params it names.
    fn actions(
        &self,
        actions: &'a BTreeMap<String, ActionData>,
        ranks: impl Fn(&str) -> u8,
        names: &mut PackageNames<'a>,
    ) -> Result<(), LoadProblem> {
        for (id, ability) in actions {
            let at = Place::Action(id.clone());
            self.kind(id, ability)?;
            self.ranked(id, ability, ranks(id))?;
            if ability.projectile.is_some() {
                self.require(Capability::Projectiles, &at)?;
            }
            if ability.area.is_some() {
                self.require(Capability::Areas, &at)?;
            }
            for modifier in ability.modifiers() {
                modifier_exists(names.modifiers, modifier, &at)?;
            }
            for filter in ability.filters() {
                self.filter_data(filter, &at)?;
            }
            self.stats_declared(ability.params.values().flat_map(Param::stats), &at)?;
            for name in ability.param_refs() {
                if !ability.params.contains_key(name) {
                    return Err(LoadProblem::Unknown {
                        of: NameKind::Param,
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
        Ok(())
    }

    /// The package's modifiers: the capability, the modifiers, filters and params each names, and
    /// the roles of their scripts. A modifier reads its own params, then those of the abilities
    /// that apply it, `appliers`.
    fn modifiers(
        &self,
        names: &mut PackageNames<'a>,
        appliers: &BTreeMap<&str, Vec<&'a ActionData>>,
    ) -> Result<(), LoadProblem> {
        for (id, modifier) in names.modifiers {
            let at = Place::Modifier(id.clone());
            self.require(Capability::Stats, &at)?;
            let scaled = modifier.params.values().flat_map(Param::stats);
            self.stats_declared(modifier.stats.keys().chain(scaled), &at)?;
            if let Some(affects) = &modifier.affects {
                self.filter_data(affects, &at)?;
            }
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
                return Err(LoadProblem::Unknown {
                    of: NameKind::Param,
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
                return Err(LoadProblem::Unknown {
                    of: NameKind::Param,
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
            let stat = Stat::named(name).ok_or_else(|| LoadProblem::Unknown {
                of: NameKind::Stat,
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
            return Err(LoadProblem::Unknown {
                of: NameKind::MarkerTag,
                at,
                name: name.clone(),
            });
        }
        let choice = |name: &String| data.choices.keys().any(|choice| choice.as_str() == name);
        if let Some(name) = facts.choices.iter().find(|name| !choice(name)) {
            return Err(LoadProblem::Choice(ChoiceProblem::UnknownChoice {
                at,
                name: name.clone(),
            }));
        }
        if let Some(kind) = facts
            .slot_kinds
            .iter()
            .find(|kind| data.slots.named(kind).is_none())
        {
            return Err(LoadProblem::Choice(ChoiceProblem::UnknownSlotKind {
                at,
                kind: kind.clone(),
            }));
        }
        let resource = |name: &String| ResourceId::of(&data.resources, name).is_some();
        if let Some(name) = facts.resources.iter().find(|name| !resource(name)) {
            return Err(LoadProblem::Unknown {
                of: NameKind::Resource,
                at,
                name: name.clone(),
            });
        }
        let pool = |name: &String| data.pools.keys().any(|pool| pool.as_str() == name);
        if let Some(name) = facts.pools.iter().find(|name| !pool(name)) {
            return Err(LoadProblem::Unknown {
                of: NameKind::Pool,
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
            return Err(LoadProblem::Unknown {
                of: NameKind::DamageKind,
                at,
                name: kind.clone(),
            });
        }
        Ok(())
    }

    /// The mode's damage kinds: with `combat`, at least one; never more than a byte tells apart.
    fn damage_kinds(&self) -> Result<(), LoadProblem> {
        let data = &self.packages.data;
        let kinds = &data.combat.damage_kinds;
        if kinds.len() > usize::from(u8::MAX) + 1 {
            return Err(LoadProblem::TooManyDamageKinds);
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
        Ok(())
    }

    /// An action of a kind the release runs, with the capability of its kind: a `cast` of
    /// `abilities`, with no weapon field; an `attack` of `combat`, with all three, a unit target,
    /// a range in meters, its stats declared and its damage kind the mode's.
    fn kind(&self, id: &str, action: &ActionData) -> Result<(), LoadProblem> {
        let at = Place::Action(id.to_owned());
        let fields = action.weapon_fields();
        match action.kind {
            ActionKind::Cast => {
                self.require(Capability::Abilities, &at)?;
                if fields.contains(&true) {
                    return Err(LoadProblem::KindField(id.to_owned()));
                }
            }
            ActionKind::Attack => {
                self.require(Capability::Combat, &at)?;
                let global = action
                    .range
                    .as_ref()
                    .is_none_or(|range| range.values().contains(&RangeField::Range(Range::Global)));
                let aims = matches!(action.targeting, Targeting::Unit(_));
                if fields.contains(&false) || global || !aims || action.cast_fields() {
                    return Err(LoadProblem::KindField(id.to_owned()));
                }
                self.stats_declared(action.rate.iter().chain(&action.damage), &at)?;
                let kinds = &self.packages.data.combat.damage_kinds;
                if let Some(kind) = action
                    .damage_kind
                    .as_ref()
                    .filter(|kind| !kinds.contains(kind))
                {
                    return Err(LoadProblem::Unknown {
                        of: NameKind::DamageKind,
                        at,
                        name: kind.to_string(),
                    });
                }
            }
            kind => {
                return Err(LoadProblem::KindNotRun {
                    action: id.to_owned(),
                    kind,
                });
            }
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
            Some(layer) if navigation.layer(layer).is_none() => Err(LoadProblem::Unknown {
                of: NameKind::Layer,
                at: at.clone(),
                name: layer.to_string(),
            }),
            _ => Ok(()),
        }
    }

    /// The mode's pools and player resources: at most `Pools::LIMIT` pools and
    /// `ResourceId::LIMIT` resources, no pool named as a resource, each pool with stats the mode
    /// declares; and with `combat`, a life pool among them.
    fn pools_and_resources(&self) -> Result<(), LoadProblem> {
        let data = &self.packages.data;
        if data.pools.len() > Pools::LIMIT {
            return Err(LoadProblem::TooManyPools);
        }
        if data.resources.len() > ResourceId::LIMIT {
            return Err(LoadProblem::TooManyResources);
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
            return Err(LoadProblem::Unknown {
                of: NameKind::Pool,
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

    /// A unit type at `at`, of a package of `actions` and `modifiers`: each capability its
    /// sections use declared, its stats and pools the mode's, its layer one the mode declares,
    /// its projectile fast enough; its slots of kinds the mode declares, each holding actions of
    /// `actions`, none twice; and its passive one of `modifiers`.
    fn unit_type(
        &self,
        unit_type: &UnitTypeFile,
        at: &Place,
        actions: &BTreeMap<String, ActionData>,
        modifiers: &BTreeMap<String, ModifierData>,
    ) -> Result<(), LoadProblem> {
        let sections = [
            (unit_type.stats.is_some(), Capability::Stats),
            (unit_type.combat.is_some(), Capability::Combat),
            (unit_type.orders.is_some(), Capability::Orders),
            (unit_type.vision.is_some(), Capability::Vision),
            (!unit_type.slots.is_empty(), Capability::Abilities),
        ];
        for (used, capability) in sections {
            if used {
                self.require(capability, at)?;
            }
        }
        if let Some(stats) = &unit_type.stats {
            self.stats_declared(stats.0.keys(), at)?;
        }
        self.unit_pools(&unit_type.pools, unit_type.combat.is_some(), at)?;
        self.collision_layer(unit_type.collision.as_ref(), at)?;
        let kinds = &self.packages.data.slots;
        let mut slotted = BTreeSet::new();
        for (kind, ids) in &unit_type.slots {
            kinds.named(kind.as_str()).ok_or_else(|| {
                LoadProblem::Choice(ChoiceProblem::UnknownSlotKind {
                    at: at.clone(),
                    kind: kind.to_string(),
                })
            })?;
            for id in ids {
                if !actions.contains_key(id) {
                    return Err(LoadProblem::UnknownSlot(id.clone()));
                }
                if !slotted.insert(id.as_str()) {
                    return Err(LoadProblem::RepeatedSlot(id.clone()));
                }
            }
        }
        if let Some(passive) = &unit_type.passive {
            modifier_exists(modifiers, passive, at)?;
        }
        Ok(())
    }

    /// The mode's slot kinds: no more than a slot's index holds, and none named twice.
    fn slot_kinds(&self) -> Result<(), LoadProblem> {
        let kinds = &self.packages.data.slots.0;
        if kinds.len() > ActionSlots::LIMIT {
            return Err(LoadProblem::Choice(ChoiceProblem::TooManySlotKinds));
        }
        let mut seen = BTreeSet::new();
        match kinds.iter().find(|kind| !seen.insert(&kind.name)) {
            Some(kind) => Err(LoadProblem::RepeatedName(kind.name.clone())),
            None => Ok(()),
        }
    }

    /// The mode's choices: a choice of loadout entries fills a slot kind the mode declares, and
    /// a choice of avatars none; every choice of loadout entries fills a kind of the same ranks,
    /// which the entries have.
    fn choices(&self) -> Result<(), LoadProblem> {
        let data = &self.packages.data;
        let mut ranks = None;
        for (name, choice) in &data.choices {
            let fills = |kind: &DeclaredName| {
                data.slots.named(kind.as_str()).ok_or_else(|| {
                    LoadProblem::Choice(ChoiceProblem::UnknownSlotKind {
                        at: Place::Choice(name.clone()),
                        kind: kind.to_string(),
                    })
                })
            };
            match (choice.offers, &choice.slot) {
                (Offers::Loadout, Some(kind)) => {
                    let kind_ranks = data.slots.ranks(fills(kind)?);
                    if *ranks.get_or_insert(kind_ranks) != kind_ranks {
                        return Err(LoadProblem::Choice(ChoiceProblem::LoadoutRanks));
                    }
                }
                (Offers::Avatars, None) => {}
                _ => return Err(LoadProblem::Choice(ChoiceProblem::ChoiceSlot(name.clone()))),
            }
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
                return Err(LoadProblem::Unknown {
                    of: NameKind::Pool,
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
    fn ranked(&self, id: &str, ability: &ActionData, ranks: u8) -> Result<(), LoadProblem> {
        let data = &self.packages.data;
        if let Some(name) = ability
            .cost_names()
            .find(|name| data.cost_target(name).is_none())
        {
            return Err(LoadProblem::Unknown {
                of: NameKind::Cost,
                at: Place::Action(id.to_owned()),
                name: name.to_string(),
            });
        }
        if !ability.check_ranks(usize::from(ranks)) {
            return Err(LoadProblem::RankCount {
                action: id.to_owned(),
                ranks,
            });
        }
        for rank in 1..=ranks {
            ability
                .fields_at(rank, |name| data.cost_target(name))
                .map_err(|field| LoadProblem::ActionField {
                    action: id.to_owned(),
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
            Some(name) => Err(LoadProblem::Unknown {
                of: NameKind::Stat,
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
            None => Err(LoadProblem::Unknown {
                of: NameKind::Filter,
                at: at.clone(),
                name: filter.to_owned(),
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
        Err(LoadProblem::Unknown {
            of: NameKind::Filter,
            at: at.clone(),
            name: filter.to_string(),
        })
    }

    /// A projectile, of `speed` meters a second, may home, so it flies faster than the cap.
    fn projectile_speed(&self, speed: Option<Scalar>, at: &Place) -> Result<(), LoadProblem> {
        match speed.map(Scalar::to_num) {
            None => Ok(()),
            Some(Some(speed)) if speed > self.cap => Ok(()),
            Some(_) => Err(LoadProblem::ProjectileNotFaster { at: at.clone() }),
        }
    }

    /// Every speed an ability's projectile may fly at, at any rank, is faster than the cap: a
    /// script may make it home.
    fn ability_projectile(&self, ability: &ActionData, at: &Place) -> Result<(), LoadProblem> {
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
                        return Err(LoadProblem::Unknown {
                            of: NameKind::Param,
                            at: at.clone(),
                            name: reference.param.clone(),
                        });
                    }
                },
            };
            for value in values {
                self.projectile_speed(Some(value), at)?;
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
    Err(LoadProblem::Unknown {
        of: NameKind::Modifier,
        at: at.to_owned(),
        name: id.to_owned(),
    })
}
