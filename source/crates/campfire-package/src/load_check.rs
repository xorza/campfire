use std::collections::{BTreeMap, BTreeSet};
use std::{iter, slice};

use campfire_capabilities::{
    ActionData, ActionDataField, ActionKind, ActionSlots, ApiOwner, ApiVersion, BookError, Books,
    CollisionData, CombatRules, DeclaredName, DeliveryData, EffectTo, Effecting, EngineTag,
    FilterData, Hook, MemberKind, ModifierData, ModifierProblem, NameKind, Number, Offers,
    PackagePath, Param, ParamProblem, Pools, Range, RangeField, ResourceId, Scalar, ScriptApi,
    ScriptRole, Stat, Targeting, TrackId, TypePlace, UnitTypeData, UnitTypeFile,
};
use campfire_math::Num;
use campfire_sim::{Capability, TickRate};

use crate::dependent::{Dependent, DependentKind};
use crate::error::{
    ChoiceProblem, CtxMisuse, DeliveryProblem, EffectProblem, Limit, LoadError, LoadProblem, Place,
    ScriptProblem,
};
use crate::mode_packages::ModePackages;
use crate::modifier_ways::{ModifierWays, Way};
use crate::package::Package;
use crate::package_view::{PackageView, ViewKind};
use crate::script_facts::{ScriptFacts, ScriptName};

/// Design 08's checks at package load, over a mode and every package it depends on: data matches
/// its schema (the reads checked that), every per-rank array has an entry for each rank, every
/// script is named by data and defines only hooks of its roles, and every capability, `ctx` name,
/// modifier, param, stat, filter and damage kind a script or data names exists for the mode. And
/// every homing projectile is faster than the mode's move speed cap.
#[derive(Debug)]
pub(crate) struct LoadCheck<'a> {
    packages: &'a ModePackages,
    /// The tags a filter may name: every tag the mode's packages name.
    tags: BTreeSet<&'a str>,
    /// The move speed cap, in meters a second.
    cap: Num,
    /// The script API of the release, which every name a script uses must be of.
    api: &'a ScriptApi,
    /// The fastest rate the mode allows, at which a time counts the most ticks.
    rate: TickRate,
    /// Every field a script may name after `.state`: those of the mode's state, and of every
    /// modifier's and unit type's of its packages, as a script's handle has no type the load
    /// knows.
    state_fields: BTreeSet<&'a str>,
}

/// The facts one package's checks share.
#[derive(Debug)]
struct PackageNames<'a> {
    package: &'a Package,
    /// What data says of each script it names.
    scripts: BTreeMap<&'a PackagePath, ScriptUse<'a>>,
    modifiers: &'a BTreeMap<DeclaredName, ModifierData>,
}

/// What data says of a script: the roles it serves, and the params its `ctx.p` may read.
#[derive(Debug, Default)]
struct ScriptUse<'a> {
    roles: BTreeSet<ScriptRole>,
    params: BTreeSet<&'a str>,
}

impl<'a> LoadCheck<'a> {
    pub(crate) fn run(packages: &'a ModePackages, api: &'a ScriptApi) -> Result<(), LoadError> {
        let manifest = &packages.manifest;
        let fail = |problem| LoadError::of(&manifest.header.name, problem);
        let mut tags = packages.tag_names();
        if EngineTag::ALL.len() + tags.len() > UnitTypeData::TAG_LIMIT {
            return Err(fail(LoadProblem::TooMany(Limit::Tags)));
        }
        tags.extend(EngineTag::ALL.map(EngineTag::name));
        // A dependency's delivery types are in its own scope, so they share no name with
        // another package's; an avatar is in the mode's, by its package's name.
        let units = &packages.content.units;
        let mut unit_types = units.len();
        for dependent in &packages.dependencies {
            unit_types += dependent.content.units.len();
            if matches!(dependent.kind, DependentKind::Avatar(_)) {
                let name = &dependent.package.header.name;
                if units.contains_key(name.as_str()) {
                    return Err(fail(LoadProblem::Repeated {
                        at: Place::UnitTypes,
                        name: name.clone(),
                    }));
                }
                unit_types += 1;
            }
        }
        if unit_types > UnitTypeData::TYPE_LIMIT {
            return Err(fail(LoadProblem::TooMany(Limit::UnitTypes)));
        }
        let every = [&packages.mode].into_iter().chain(
            packages
                .dependencies
                .iter()
                .map(|dependent| &dependent.package),
        );
        for package in every {
            if !ApiVersion::RELEASE.loads(package.header.api) {
                let problem = LoadProblem::OtherApi(package.header.api);
                return Err(LoadError::of(&package.header.name, problem));
            }
        }
        let check = LoadCheck {
            packages,
            tags,
            cap: manifest.max_move_speed.get(),
            api,
            rate: TickRate::new(manifest.tick_hz.fastest()),
            state_fields: LoadCheck::state_fields(packages),
        };
        check.mode().map_err(fail)?;
        check.loadout()?;
        for (view, dependent) in packages.packages().skip(1).zip(&packages.dependencies) {
            check
                .dependent(view, dependent)
                .map_err(|problem| LoadError::of(&dependent.package.header.name, problem))?;
        }
        packages
            .stat_graph()
            .order()
            .map_err(|stats| fail(LoadProblem::StatLoop(stats)))?;
        let input = packages.book_input(check.rate);
        Books::build(&input).map_err(|error| check.book_error(error))?;
        // After the build, which resolved the map's names.
        check.map_walkable().map_err(fail)?;
        Ok(())
    }

    /// The load error of what building the books refused, at the fastest rate the mode allows.
    fn book_error(&self, error: BookError) -> LoadError {
        let packages = self.packages;
        let at = |package: u16, unit_type: TypePlace| match unit_type {
            TypePlace::Avatar => {
                let view = packages.packages().nth(usize::from(package));
                let package = &view.expect("the books name a package").package;
                Place::Avatar(package.header.name.clone())
            }
            TypePlace::Declared(name) => Place::UnitType(name),
        };
        let (package, problem) = match error {
            BookError::Kit {
                package,
                unit_type,
                error,
            } => (
                package,
                LoadProblem::UnitKit {
                    at: at(package, unit_type),
                    error,
                },
            ),
            BookError::Ai {
                package,
                unit_type,
                error,
            } => (
                package,
                LoadProblem::Ai {
                    at: at(package, unit_type),
                    error,
                },
            ),
            BookError::Action {
                package,
                action,
                error,
            } => (package, LoadProblem::Action { action, error }),
            BookError::Modifier {
                package,
                modifier,
                problem,
            } => (package, LoadProblem::Modifier { modifier, problem }),
            BookError::Mode(error) => (0, LoadProblem::Mode(error)),
            BookError::AreaTime { package, unit_type } => (
                package,
                LoadProblem::Delivery(DeliveryProblem::AreaTime(unit_type)),
            ),
        };
        let view = packages.packages().nth(usize::from(package));
        LoadError::of(
            &view.expect("the books name a package").package.header.name,
            problem,
        )
    }

    /// The map can be walked by every unit that walks, among the mode's unit types and its
    /// avatars, as `MapData::check_walkable` sets: the widest of each layer stands on every
    /// marker's point and waypoint, and reaches every waypoint from the one before, among the
    /// map's placed units that cannot walk.
    fn map_walkable(&self) -> Result<(), LoadProblem> {
        let packages = self.packages;
        let walkers = packages.walkers();
        let body_of = |unit_type: &str| {
            let unit_type = packages.content.units.get(unit_type)?;
            let body = packages.data.navigation.body(unit_type.collision.as_ref());
            body.filter(|_| !unit_type.walks())
        };
        packages
            .map
            .check_walkable(&walkers, body_of)
            .map_err(LoadProblem::Map)
    }

    /// The mode package: its unit types' sections, its map, its modifiers and its scripts.
    fn mode(&self) -> Result<(), LoadProblem> {
        let packages = self.packages;
        let data = &packages.data;
        let content = &packages.content;
        let units = &content.units;
        for (list, at) in [
            (&data.combat.damage_kinds, Place::Combat),
            (&data.resources, Place::Resources),
            (&data.navigation.layers, Place::Navigation),
        ] {
            let mut seen = BTreeSet::new();
            if let Some(name) = list.iter().find(|&name| !seen.insert(name)) {
                let name = name.to_string();
                return Err(LoadProblem::Repeated { at, name });
            }
        }
        self.damage_kinds()?;
        self.pools_and_resources()?;
        self.layers()?;
        self.tracks()?;
        if data.combat.stats().next().is_some() || data.combat.life.is_some() {
            self.require(Capability::Combat, &Place::Combat)?;
            self.stats_declared(data.combat.stats(), &Place::Combat)?;
        }
        self.slot_kinds()?;
        self.choices()?;
        for (name, unit_type) in units {
            let at = Place::UnitType(name.clone());
            self.unit_type(unit_type, &at, &content.actions, &content.modifiers)?;
        }
        let held = units.iter().filter_map(|(name, unit_type)| {
            Some((Place::UnitType(name.clone()), unit_type.passive.as_ref()?))
        });
        passives_held_once(held.chain(action_passives(&content.actions)))?;
        if !packages.map.paths.is_empty() {
            self.require(Capability::Navigation, &Place::Paths)?;
        }
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
        let mut names = PackageNames::new(&packages.mode, &content.modifiers);
        let mode_params: BTreeSet<&str> = data.params.keys().map(DeclaredName::as_str).collect();
        names.serve(&data.script, ScriptRole::Mode, mode_params.iter().copied());
        for unit_type in units.values() {
            if let Some(orders) = &unit_type.orders {
                names.serve(&orders.ai, ScriptRole::Ai, mode_params.iter().copied());
            }
        }
        let ranks = self.slotted_ranks(units.values())?;
        let ranks = |id: &str| ranks.get(id).copied().unwrap_or(1);
        self.actions(&content.actions, units, ranks, &mut names)?;
        let view = packages.packages().next().expect("the mode, first");
        let ways = packages.modifier_ways(view);
        self.modifiers(&mut names, &ways, &content.actions, ranks)?;
        self.scripts(&names)
    }

    /// No entry id is held by two loadout packages, as players choose loadout entries by id.
    fn loadout(&self) -> Result<(), LoadError> {
        let mut seen = BTreeSet::new();
        for dependent in &self.packages.dependencies {
            if !matches!(dependent.kind, DependentKind::Loadout) {
                continue;
            }
            let actions = dependent.content.actions.keys();
            if let Some(id) = actions.clone().find(|&id| !seen.insert(id)) {
                let problem = LoadProblem::Repeated {
                    at: Place::Loadouts,
                    name: id.to_string(),
                };
                return Err(LoadError::of(&dependent.package.header.name, problem));
            }
        }
        Ok(())
    }

    /// An avatar or loadout package the mode depends on: an avatar's unit type, with every
    /// action of the package in its slots, and each action with the ranks of its kind; a
    /// loadout's actions, each with the ranks of the slot kind its choice fills; and its delivery
    /// types.
    fn dependent(
        &self,
        view: PackageView<'a>,
        dependent: &'a Dependent,
    ) -> Result<(), LoadProblem> {
        let package = &dependent.package;
        let content = &dependent.content;
        let (actions, modifiers) = (&content.actions, &content.modifiers);
        let slotted = match &dependent.kind {
            DependentKind::Avatar(avatar) => {
                let at = Place::Avatar(package.header.name.clone());
                if !package.text.gives(avatar.name.as_str()) {
                    return Err(LoadProblem::Unknown {
                        of: NameKind::Message,
                        at,
                        name: avatar.name.to_string(),
                    });
                }
                if avatar.unit.orders.is_some() {
                    return Err(LoadProblem::AvatarOrders);
                }
                if avatar.unit.delivers() {
                    return Err(LoadProblem::Delivery(DeliveryProblem::NotDelivery(at)));
                }
                self.unit_type(&avatar.unit, &at, actions, modifiers)?;
                let held = avatar.unit.passive.as_ref().map(|passive| (at, passive));
                passives_held_once(held.into_iter().chain(action_passives(actions)))?;
                let ranks = self.slotted_ranks([&avatar.unit])?;
                let unslotted = actions.keys().find(|id| !ranks.contains_key(id.as_str()));
                if let Some(id) = unslotted {
                    return Err(LoadProblem::Unslotted(id.clone()));
                }
                Some(ranks)
            }
            DependentKind::Loadout => {
                passives_held_once(action_passives(actions))?;
                None
            }
        };
        for (id, unit_type) in &content.units {
            let at = Place::UnitType(id.clone());
            if !unit_type.delivery_only() {
                return Err(LoadProblem::Delivery(DeliveryProblem::NotDelivery(at)));
            }
            self.unit_type(unit_type, &at, actions, modifiers)?;
        }
        let loadout_ranks = self.packages.data.loadout_ranks();
        let ranks = |id: &str| slotted.as_ref().map_or(loadout_ranks, |ranks| ranks[id]);
        let mut names = PackageNames::new(package, modifiers);
        self.actions(actions, &content.units, ranks, &mut names)?;
        let ways = self.packages.modifier_ways(view);
        self.modifiers(&mut names, &ways, actions, ranks)?;
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
            .map_err(|id| LoadProblem::ActionRanks(id.clone()))
    }

    /// A package's `actions`, each with the ranks `ranks` gives it and its delivery one of the
    /// package's `units`, and the roles of their scripts in `names`: the capabilities each uses,
    /// and the modifiers, filters, stats and params it names.
    fn actions(
        &self,
        actions: &'a BTreeMap<DeclaredName, ActionData>,
        units: &BTreeMap<DeclaredName, UnitTypeFile>,
        ranks: impl Fn(&str) -> u8,
        names: &mut PackageNames<'a>,
    ) -> Result<(), LoadProblem> {
        for (id, ability) in actions {
            let at = Place::Action(id.clone());
            self.kind(id, ability)?;
            self.ranked(id, ability, ranks(id.as_str()))?;
            self.effects(id, ability)?;
            if let Some(delivery) = &ability.delivery {
                let capability = match delivery {
                    DeliveryData::Projectile { .. } => Capability::Projectiles,
                    DeliveryData::Area { .. } => Capability::Areas,
                };
                self.require(capability, &at)?;
                delivery_holds(id, ability, delivery, units)?;
            }
            for modifier in ability.modifiers() {
                modifier_exists(names.modifiers, modifier.as_str(), &at)?;
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
                        name: name.to_string(),
                    });
                }
            }
            if let Some(script) = &ability.script {
                names.serve(
                    script,
                    ScriptRole::Action,
                    ability.params.keys().map(DeclaredName::as_str),
                );
            }
        }
        Ok(())
    }

    /// The package's modifiers: the capability, the modifiers, filters and params each names, and
    /// the roles of their scripts. A modifier reads its own params, then those each of its `ways`
    /// gives, of `actions` at the ranks `ranks` gives them.
    fn modifiers(
        &self,
        names: &mut PackageNames<'a>,
        ways: &ModifierWays<'_>,
        actions: &'a BTreeMap<DeclaredName, ActionData>,
        ranks: impl Fn(&str) -> u8,
    ) -> Result<(), LoadProblem> {
        for (id, modifier) in names.modifiers {
            let at = Place::Modifier(id.clone());
            self.require(Capability::Stats, &at)?;
            own_tags(&modifier.tags, &at)?;
            let scaled = modifier.params.values().flat_map(Param::stats);
            self.stats_declared(modifier.stats.keys().chain(scaled), &at)?;
            if let Some(affects) = &modifier.affects {
                self.filter_data(affects, &at)?;
            }
            if let Some(aura) = &modifier.aura {
                modifier_exists(names.modifiers, aura.modifier.as_str(), &at)?;
                self.filter_data(&aura.affects, &at)?;
            }
            let by: Vec<&ActionData> = ways.actions_of(id.as_str(), actions).collect();
            let ways: Vec<&Way> = ways.of_modifier(id.as_str()).collect();
            let script = modifier
                .script
                .as_ref()
                .and_then(|path| names.package.script(path));
            let scripted = script
                .into_iter()
                .flat_map(|script| &script.facts.params)
                .filter_map(|name| DeclaredName::new(name));
            let reads: BTreeSet<DeclaredName> =
                modifier.param_refs().cloned().chain(scripted).collect();
            for param in &reads {
                // A param nothing declares is unknown, not a way's to give: where the modifier's
                // numbers read it, here; where its script does, as the script's checks find it.
                let declared = modifier.params.contains_key(param)
                    || by.iter().any(|action| action.params.contains_key(param));
                if !declared {
                    if modifier.param_refs().any(|read| read == param) {
                        return Err(LoadProblem::Unknown {
                            of: NameKind::Param,
                            at,
                            name: param.to_string(),
                        });
                    }
                    continue;
                }
                let number = modifier.param_refs().any(|read| read == param);
                self.modifier_param(modifier, param, number, &ways, actions)
                    .map_err(|(way, problem)| LoadProblem::ModifierParam {
                        modifier: id.clone(),
                        param: param.clone(),
                        way: way.cloned(),
                        problem,
                    })?;
            }
            for (param, own) in &modifier.params {
                let short = own.ranks().and_then(|held| {
                    ways.iter().copied().find(|way| match way {
                        Way::Action(action) => held < usize::from(ranks(action.as_str())),
                        Way::NoAction => false,
                    })
                });
                if let Some(way) = short {
                    return Err(LoadProblem::ModifierParam {
                        modifier: id.clone(),
                        param: param.clone(),
                        way: Some(way.clone()),
                        problem: ParamProblem::Short,
                    });
                }
            }
            let readable: BTreeSet<&str> = modifier
                .params
                .keys()
                .chain(by.iter().flat_map(|ability| ability.params.keys()))
                .map(DeclaredName::as_str)
                .collect();
            if negative_radius_or_shield(modifier, &by) {
                return Err(LoadProblem::Modifier {
                    modifier: id.clone(),
                    problem: ModifierProblem::Negative,
                });
            }
            if let Some(script) = &modifier.script {
                names.serve(script, ScriptRole::Modifier, readable.iter().copied());
            }
        }
        Ok(())
    }

    /// The param `param` that `modifier` reads, a `number` of it or only its script, as each of
    /// `ways` gives it: its own, whatever applies it, or else each way's action's, which no way
    /// with no action has. What a number reads holds a number at every rank, and where a time
    /// reads it, no scaling param and a time that counts in ticks; a failure is of the way, none
    /// for the modifier's own param.
    fn modifier_param<'w>(
        &self,
        modifier: &ModifierData,
        param: &DeclaredName,
        number: bool,
        ways: &[&'w Way],
        actions: &BTreeMap<DeclaredName, ActionData>,
    ) -> Result<(), (Option<&'w Way>, ParamProblem)> {
        let time = modifier.times().any(|number| number.param() == Some(param));
        let holds = |value: &Param| {
            if !number {
                return Ok(());
            }
            let numbers: Option<Vec<Num>> = param_numbers(value).collect();
            let numbers = numbers.ok_or(ParamProblem::Overflow)?;
            if time && matches!(value, Param::Scaling(_)) {
                return Err(ParamProblem::ScalingTime);
            }
            let counts = |&ms: &Num| ModifierData::ticks(ms, self.rate).is_some();
            if time && !numbers.iter().all(counts) {
                return Err(ParamProblem::Time);
            }
            Ok(())
        };
        if let Some(own) = modifier.params.get(param) {
            return holds(own).map_err(|problem| (None, problem));
        }
        for &way in ways {
            let given = match way {
                Way::Action(action) => actions
                    .get(action)
                    .and_then(|action| action.params.get(param)),
                Way::NoAction => None,
            };
            let given = given.ok_or((Some(way), ParamProblem::Missing))?;
            holds(given).map_err(|problem| (Some(way), problem))?;
        }
        Ok(())
    }

    /// Every script of the package: named by data, its hooks those of its roles, and every name
    /// it uses one the mode has.
    fn scripts(&self, names: &PackageNames<'_>) -> Result<(), LoadProblem> {
        let package = names.package;
        for &path in names.scripts.keys() {
            if package.script(path).is_none() {
                return Err(LoadProblem::Script {
                    path: path.clone(),
                    problem: ScriptProblem::Missing,
                });
            }
        }
        for script in &package.scripts {
            let path = &script.path;
            let Some(ScriptUse { roles, params }) = names.scripts.get(path) else {
                return Err(LoadProblem::Script {
                    path: path.clone(),
                    problem: ScriptProblem::Unreferenced,
                });
            };
            let at = Place::Script(path.clone());
            let facts = &script.facts;
            let fail = |problem| LoadProblem::Script {
                path: path.clone(),
                problem,
            };
            let misuse = |misuse| fail(ScriptProblem::CtxMisuse(misuse));
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
                    return Err(fail(ScriptProblem::UnknownHook(function.name.clone())));
                }
                let Some(hook) = hook else {
                    continue;
                };
                if !roles.contains(&hook.role()) || hook.params() != function.params {
                    return Err(fail(ScriptProblem::UnknownHook(function.name.clone())));
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
                    return Err(fail(ScriptProblem::UnknownCtx(used.name.clone())));
                };
                if let Some(capability) = member.capability {
                    self.require(capability, &at)?;
                }
            }
            for used in &facts.members {
                if !self.member_known(facts, &used.name, used.kind) {
                    return Err(fail(ScriptProblem::UnknownMember(used.name.clone())));
                }
            }
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
            for named in &facts.names {
                self.script_name(named, names.modifiers, &at)?;
            }
            if let Some(field) = facts
                .state_fields
                .iter()
                .find(|field| !self.state_fields.contains(field.as_str()))
            {
                return Err(fail(ScriptProblem::UnknownState(field.clone())));
            }
        }
        Ok(())
    }

    /// Every state field the match's packages declare: the mode's, and every modifier's and unit
    /// type's, avatars among them.
    fn state_fields(packages: &'a ModePackages) -> BTreeSet<&'a str> {
        let mut fields: BTreeSet<&str> = packages
            .data
            .state
            .keys()
            .map(DeclaredName::as_str)
            .collect();
        for view in packages.packages() {
            let content = view.content;
            let modifiers = content
                .modifiers
                .values()
                .flat_map(|modifier| modifier.state.keys());
            let avatar = match view.kind {
                ViewKind::Avatar(avatar) => Some(&avatar.unit),
                ViewKind::Mode | ViewKind::Loadout => None,
            };
            let units = content.units.values().chain(avatar);
            let units = units.flat_map(|unit| unit.core.state.keys());
            fields.extend(modifiers.chain(units).map(DeclaredName::as_str));
        }
        fields
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

    /// A name a script at `at` gives an argument of a name kind is one of its kind that the match
    /// has: a modifier one of the script's package's, `modifiers`; a unit type one of the mode's
    /// scope.
    fn script_name(
        &self,
        named: &ScriptName,
        modifiers: &BTreeMap<DeclaredName, ModifierData>,
        at: &Place,
    ) -> Result<(), LoadProblem> {
        let packages = self.packages;
        let data = &packages.data;
        let name = named.name.as_str();
        let known = match named.kind {
            NameKind::Modifier => return modifier_exists(modifiers, name, at),
            NameKind::Filter => return self.filter_text(name, at),
            NameKind::Stat => {
                let stat = Stat::named(name).ok_or_else(|| named.unknown(at))?;
                return self.stats_declared([&stat], at);
            }
            NameKind::Choice => {
                if declares(data.choices.keys(), name) {
                    return Ok(());
                }
                return Err(LoadProblem::Choice(ChoiceProblem::UnknownChoice {
                    at: at.clone(),
                    name: named.name.clone(),
                }));
            }
            NameKind::SlotKind => {
                if data.slots.named(name).is_some() {
                    return Ok(());
                }
                return Err(LoadProblem::Choice(ChoiceProblem::UnknownSlotKind {
                    at: at.clone(),
                    kind: named.name.clone(),
                }));
            }
            NameKind::MarkerTag => packages
                .map
                .markers
                .iter()
                .any(|marker| declares(marker.tags.iter(), name)),
            NameKind::Resource => ResourceId::named(&data.resources, name).is_some(),
            NameKind::Pool => declares(data.pools.keys(), name),
            NameKind::DamageKind => declares(data.combat.damage_kinds.iter(), name),
            NameKind::Track => declares(data.tracks.keys(), name),
            NameKind::Tag => self.tags.contains(name),
            NameKind::Team => {
                let teams = &packages.manifest.teams;
                declares(teams.iter().map(|team| &team.name), name)
            }
            NameKind::Path => declares(packages.map.paths.iter().map(|path| &path.name), name),
            NameKind::UnitType => {
                packages.content.units.contains_key(name)
                    || packages.avatar_names().any(|avatar| avatar == name)
            }
            NameKind::Param | NameKind::Cost | NameKind::Layer | NameKind::Message => {
                unreachable!("no argument of the script API is a {}", named.kind)
            }
        };
        if known {
            Ok(())
        } else {
            Err(named.unknown(at))
        }
    }

    /// The mode's damage kinds: with `combat`, at least one; never more than a match holds.
    const fn damage_kinds(&self) -> Result<(), LoadProblem> {
        let data = &self.packages.data;
        let kinds = &data.combat.damage_kinds;
        if kinds.len() > CombatRules::DAMAGE_KIND_LIMIT {
            return Err(LoadProblem::TooMany(Limit::DamageKinds));
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

    /// An action of a kind the release runs, with the capability of its kind and the fields the
    /// table of action fields lets its kind take: a `cast` of `abilities`; an `attack` of
    /// `combat`, aimed at a unit, of a range in meters, its stats declared and its damage kind
    /// the mode's; a `train` of `production`, aimed at nothing, of a unit type of the mode's
    /// that stands.
    fn kind(&self, id: &DeclaredName, action: &ActionData) -> Result<(), LoadProblem> {
        let at = Place::Action(id.clone());
        let run = matches!(
            action.kind,
            ActionKind::Cast | ActionKind::Attack | ActionKind::Train
        );
        if run && let Some(field) = ActionDataField::misused(action, action.kind) {
            return Err(LoadProblem::KindField {
                action: id.to_owned(),
                field,
            });
        }
        match action.kind {
            ActionKind::Cast => {
                self.require(Capability::Abilities, &at)?;
            }
            ActionKind::Train => {
                self.require(Capability::Production, &at)?;
                if action.targeting != Targeting::None {
                    return Err(LoadProblem::TrainAims(id.to_owned()));
                }
                let unit_types = &self.packages.content.units;
                let name = action
                    .unit_type
                    .as_ref()
                    .expect("a train needs its unit type");
                match unit_types.get(name) {
                    None => {
                        return Err(LoadProblem::Unknown {
                            of: NameKind::UnitType,
                            at,
                            name: name.to_string(),
                        });
                    }
                    Some(unit_type) if unit_type.delivers() => {
                        return Err(LoadProblem::Delivery(DeliveryProblem::Trained(
                            id.to_owned(),
                        )));
                    }
                    Some(_) => {}
                }
            }
            ActionKind::Attack => {
                self.require(Capability::Combat, &at)?;
                let global = action.range.as_ref().is_some_and(|range| {
                    range.values().contains(&RangeField::Range(Range::Global))
                });
                if !matches!(action.targeting, Targeting::Unit(_)) {
                    return Err(LoadProblem::AttackAims(id.to_owned()));
                }
                if global {
                    return Err(LoadProblem::GlobalAttack(id.to_owned()));
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
                    action: id.clone(),
                    kind,
                });
            }
        }
        Ok(())
    }

    /// The effect lists of `action`, whose id is `id`: each effect one the release runs, of a
    /// capability the mode declares and a name it declares, in a list that runs, to a unit the
    /// list reaches; and each number at least 0 and a sim number at every rank, a duration whole
    /// milliseconds within a `u32`. The modifiers and params they name, the action's checks find.
    fn effects(&self, id: &DeclaredName, action: &ActionData) -> Result<(), LoadProblem> {
        let data = &self.packages.data;
        let at = Place::Action(id.clone());
        for (list, effects) in action.effect_lists() {
            let fail = |problem| LoadProblem::Effect {
                action: id.clone(),
                list,
                problem,
            };
            if !effects.is_empty() && list != Hook::OnResolve && action.delivery.is_none() {
                return Err(fail(EffectProblem::NoDelivery));
            }
            let reaches = match list {
                Hook::OnResolve => matches!(action.targeting, Targeting::Unit(_)),
                _ => list == Hook::OnHit,
            };
            for effect in effects {
                if effect.to == EffectTo::Reached && !reaches {
                    return Err(fail(EffectProblem::NoUnit));
                }
                let unknown = |of, name: &DeclaredName| LoadProblem::Unknown {
                    of,
                    at: at.clone(),
                    name: name.to_string(),
                };
                match &effect.does {
                    Effecting::Planned(planned) => {
                        return Err(fail(EffectProblem::Planned(*planned)));
                    }
                    Effecting::Damage { kind, .. } => {
                        self.require(Capability::Combat, &at)?;
                        if !data.combat.damage_kinds.contains(kind) {
                            return Err(unknown(NameKind::DamageKind, kind));
                        }
                    }
                    Effecting::Heal { .. } => self.require(Capability::Combat, &at)?,
                    Effecting::Restore { pool, .. } => {
                        self.require(Capability::Combat, &at)?;
                        if !data.pools.contains_key(pool) {
                            return Err(unknown(NameKind::Pool, pool));
                        }
                    }
                    Effecting::Modifier { duration_ms, .. } => {
                        self.require(Capability::Stats, &at)?;
                        if let Some(duration) = duration_ms
                            && !whole_ms(action, duration)
                        {
                            return Err(fail(EffectProblem::Duration));
                        }
                    }
                    Effecting::Xp { track, .. } => {
                        self.require(Capability::Progression, &at)?;
                        if !data.tracks.contains_key(track) {
                            return Err(unknown(NameKind::Track, track));
                        }
                    }
                }
                for number in effect.does.numbers() {
                    number_holds(action, number).map_err(fail)?;
                }
            }
        }
        Ok(())
    }

    /// The mode's tracks are progression's: no more than a unit holds, and at most one the
    /// `level` track.
    fn tracks(&self) -> Result<(), LoadProblem> {
        let tracks = &self.packages.data.tracks;
        if tracks.is_empty() {
            return Ok(());
        }
        self.require(Capability::Progression, &Place::Tracks)?;
        if tracks.len() > TrackId::LIMIT {
            return Err(LoadProblem::TooMany(Limit::Tracks));
        }
        if tracks.values().filter(|track| track.level).count() > 1 {
            return Err(LoadProblem::LevelTracks);
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
            Some(layer) if navigation.layer_named(layer).is_none() => Err(LoadProblem::Unknown {
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
            return Err(LoadProblem::TooMany(Limit::Pools));
        }
        if data.resources.len() > ResourceId::LIMIT {
            return Err(LoadProblem::TooMany(Limit::Resources));
        }
        if let Some(name) = data
            .resources
            .iter()
            .find(|name| data.pools.contains_key(*name))
        {
            return Err(LoadProblem::Repeated {
                at: Place::Resources,
                name: name.to_string(),
            });
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
    /// sections use declared, its stats and pools the mode's, its layer one the mode declares;
    /// its slots of kinds the mode declares, each holding actions of `actions`, none twice; its
    /// passive one of `modifiers`; and a projectile or an area type a delivery type alone: a
    /// homing projectile faster than the cap, its filter of the match's tags, and an area's
    /// `inside` modifiers of `modifiers`.
    fn unit_type(
        &self,
        unit_type: &UnitTypeFile,
        at: &Place,
        actions: &BTreeMap<DeclaredName, ActionData>,
        modifiers: &BTreeMap<DeclaredName, ModifierData>,
    ) -> Result<(), LoadProblem> {
        let sections = [
            (unit_type.stats.is_some(), Capability::Stats),
            (unit_type.combat.is_some(), Capability::Combat),
            (unit_type.orders.is_some(), Capability::Orders),
            (unit_type.vision.is_some(), Capability::Vision),
            (!unit_type.slots.is_empty(), Capability::Abilities),
            (!unit_type.tracks.is_empty(), Capability::Progression),
            (unit_type.production.is_some(), Capability::Production),
            (unit_type.projectile.is_some(), Capability::Projectiles),
            (unit_type.area.is_some(), Capability::Areas),
        ];
        for (used, capability) in sections {
            if used {
                self.require(capability, at)?;
            }
        }
        own_tags(&unit_type.core.tags, at)?;
        if unit_type.delivers() && !unit_type.delivery_only() {
            return Err(LoadProblem::Delivery(DeliveryProblem::NotDelivery(
                at.clone(),
            )));
        }
        if let Some(area) = &unit_type.area {
            if let Some(affects) = &area.affects {
                self.filter_data(affects, at)?;
            }
            for modifier in area.inside.modifiers() {
                modifier_exists(modifiers, modifier.as_str(), at)?;
            }
        }
        if let Some(projectile) = &unit_type.projectile {
            if projectile.homing && projectile.speed <= self.cap {
                return Err(LoadProblem::Delivery(DeliveryProblem::NotFaster(
                    at.clone(),
                )));
            }
            if let Some(hits) = &projectile.hits {
                self.filter_data(hits, at)?;
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
                    return Err(LoadProblem::Repeated {
                        at: at.clone(),
                        name: id.to_string(),
                    });
                }
                if actions[id].kind == ActionKind::Train && unit_type.production.is_none() {
                    return Err(LoadProblem::NoQueue(id.clone()));
                }
            }
        }
        if let Some(passive) = &unit_type.passive {
            modifier_exists(modifiers, passive.as_str(), at)?;
        }
        let tracks = &self.packages.data.tracks;
        if let Some(track) = unit_type
            .tracks
            .iter()
            .find(|&track| !tracks.contains_key(track))
        {
            return Err(LoadProblem::Unknown {
                of: NameKind::Track,
                at: at.clone(),
                name: track.to_string(),
            });
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
            Some(kind) => Err(LoadProblem::Repeated {
                at: Place::SlotKinds,
                name: kind.name.to_string(),
            }),
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
    /// them exactly when it has `combat`: with it, it dies at zero life; without it, it would
    /// stay a target at zero life forever.
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
                return Err(LoadProblem::Repeated {
                    at: at.clone(),
                    name: name.to_string(),
                });
            }
        }
        let lives = data
            .combat
            .life
            .as_ref()
            .is_some_and(|life| pools.contains(life));
        if lives && !combat {
            return Err(LoadProblem::CombatMissing(at.clone()));
        }
        Ok(())
    }

    /// Every per-rank array of `ability` has `ranks` entries, each pool its cost names is one
    /// the mode declares, and each capability field holds at every rank.
    fn ranked(
        &self,
        id: &DeclaredName,
        ability: &ActionData,
        ranks: u8,
    ) -> Result<(), LoadProblem> {
        let data = &self.packages.data;
        if let Some(name) = ability
            .cost_names()
            .find(|name| data.cost_target_named(name).is_none())
        {
            return Err(LoadProblem::Unknown {
                of: NameKind::Cost,
                at: Place::Action(id.clone()),
                name: name.to_string(),
            });
        }
        if !ability.check_ranks(usize::from(ranks)) {
            return Err(LoadProblem::RankCount {
                action: id.clone(),
                ranks,
            });
        }
        for rank in 1..=ranks {
            ability
                .fields_at(rank, |name| data.cost_target_named(name))
                .map_err(|field| LoadProblem::ActionField {
                    action: id.clone(),
                    field,
                })?;
        }
        Ok(())
    }

    /// Every stat in `stats` is one the mode declares.
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

    /// A script's filter: a relation, such as `enemies`, with its tags, if any, after a `:`, each
    /// one the mode's packages name.
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
}

impl<'a> PackageNames<'a> {
    const fn new(
        package: &'a Package,
        modifiers: &'a BTreeMap<DeclaredName, ModifierData>,
    ) -> PackageNames<'a> {
        PackageNames {
            package,
            scripts: BTreeMap::new(),
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
        let used = self.scripts.entry(script).or_default();
        used.roles.insert(role);
        used.params.extend(params);
    }
}

/// The values `number` of `action` can have, each rank's, as numbers, `None` for one past what
/// a number holds: its own, or its param's. None for a param the action does not declare, which
/// the action's checks refuse.
fn number_values<'n>(
    action: &'n ActionData,
    number: &'n Number,
) -> impl Iterator<Item = Option<Num>> + 'n {
    let (value, param) = match number {
        Number::Value(value) => (Some(value.to_num()), None),
        Number::Param(reference) => (None, action.params.get(&reference.param)),
    };
    value
        .into_iter()
        .chain(param.into_iter().flat_map(param_numbers))
}

/// The values `param` can have, each rank's, as numbers, `None` for one past what a number
/// holds; a scaling param's base's, as its source's stats add to it only as it applies.
fn param_numbers(param: &Param) -> impl Iterator<Item = Option<Num>> + '_ {
    let (ranked, base): (&[Scalar], &[Num]) = match param {
        Param::Ranked(ranked) => (ranked.values(), &[]),
        Param::Scaling(scaling) => (&[], scaling.base.values()),
    };
    let ranked = ranked.iter().map(|value| value.to_num());
    ranked.chain(base.iter().copied().map(Some))
}

/// Whether the aura radius or the shield of `modifier` is negative: as a value, or at a rank of
/// its own param, or, for a param it does not declare, of the param of an action `by` that
/// applies it.
fn negative_radius_or_shield(modifier: &ModifierData, by: &[&ActionData]) -> bool {
    let negative = |value: Option<Num>| value.is_some_and(|value| value < Num::ZERO);
    let numbers = [
        modifier.aura.as_ref().map(|aura| &aura.radius),
        modifier.shield.as_ref(),
    ];
    numbers.into_iter().flatten().any(|number| match number {
        Number::Value(value) => negative(value.to_num()),
        Number::Param(reference) => match modifier.params.get(&reference.param) {
            Some(param) => param_numbers(param).any(negative),
            None => by
                .iter()
                .filter_map(|action| action.params.get(&reference.param))
                .any(|param| param_numbers(param).any(negative)),
        },
    })
}

/// An effect's number: at least 0 and a sim number at every rank.
fn number_holds(action: &ActionData, number: &Number) -> Result<(), EffectProblem> {
    for value in number_values(action, number) {
        if value.ok_or(EffectProblem::Overflow)? < Num::ZERO {
            return Err(EffectProblem::Negative);
        }
    }
    Ok(())
}

/// A modifier's duration: whole milliseconds within a `u32` at every rank, and no scaling param,
/// whose value only a call knows.
fn whole_ms(action: &ActionData, duration: &Number) -> bool {
    let values = match duration {
        Number::Value(value) => slice::from_ref(value),
        Number::Param(reference) => match action.params.get(&reference.param) {
            Some(Param::Ranked(ranked)) => ranked.values(),
            Some(Param::Scaling(_)) => return false,
            None => &[],
        },
    };
    let whole = |value: &Scalar| matches!(value, Scalar::Int(ms) if u32::try_from(*ms).is_ok());
    values.iter().all(whole)
}

/// The passive of each of `actions`, by its place.
fn action_passives(
    actions: &BTreeMap<DeclaredName, ActionData>,
) -> impl Iterator<Item = (Place, &DeclaredName)> {
    actions.iter().filter_map(|(id, action)| {
        Some((Place::Action(id.clone()), action.passive_modifier.as_ref()?))
    })
}

/// Each modifier the passive of one of `owners` at most, the unit types and actions of one
/// package, each with the modifier it holds as its passive.
fn passives_held_once<'p>(
    owners: impl IntoIterator<Item = (Place, &'p DeclaredName)>,
) -> Result<(), LoadProblem> {
    let mut held: BTreeMap<&DeclaredName, Place> = BTreeMap::new();
    for (owner, modifier) in owners {
        if let Some(first) = held.get(modifier) {
            return Err(LoadProblem::SharedPassive {
                modifier: modifier.clone(),
                owners: [first.clone(), owner],
            });
        }
        held.insert(modifier, owner);
    }
    Ok(())
}

/// The tags a unit type or a modifier at `at` carries: none the engine's, which only the engine
/// gives.
fn own_tags(tags: &[DeclaredName], at: &Place) -> Result<(), LoadProblem> {
    match tags.iter().find_map(|tag| EngineTag::named(tag.as_str())) {
        Some(tag) => Err(LoadProblem::EngineTag {
            at: at.clone(),
            tag,
        }),
        None => Ok(()),
    }
}

/// `id` is one of `modifiers`.
fn modifier_exists(
    modifiers: &BTreeMap<DeclaredName, ModifierData>,
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

/// The `delivery` of `action`, whose id is `id`: a unit type of its package's `units` with the
/// section of its kind. A projectile needs an aim, and homes alone and at a unit; an area lands
/// on a point, a unit or the caster, not along a direction; a weapon's is a homing projectile.
fn delivery_holds(
    id: &DeclaredName,
    action: &ActionData,
    delivery: &DeliveryData,
    units: &BTreeMap<DeclaredName, UnitTypeFile>,
) -> Result<(), LoadProblem> {
    let fail = |problem: fn(DeclaredName) -> DeliveryProblem| {
        Err(LoadProblem::Delivery(problem(id.clone())))
    };
    let name = delivery.unit_type();
    let unit_type = units.get(name).ok_or_else(|| LoadProblem::Unknown {
        of: NameKind::UnitType,
        at: Place::Action(id.clone()),
        name: name.to_string(),
    })?;
    let wrong_section = || {
        Err(LoadProblem::Delivery(DeliveryProblem::WrongSection {
            action: id.clone(),
            unit_type: name.clone(),
        }))
    };
    let homing = match delivery {
        DeliveryData::Projectile { count, .. } => {
            let Some(projectile) = &unit_type.projectile else {
                return wrong_section();
            };
            let at_unit = matches!(action.targeting, Targeting::Unit(_));
            if action.targeting == Targeting::None {
                return fail(DeliveryProblem::NoAim);
            }
            if projectile.homing && (!at_unit || count.get() > 1) {
                return fail(DeliveryProblem::Homing);
            }
            projectile.homing
        }
        DeliveryData::Area { .. } => {
            if unit_type.area.is_none() {
                return wrong_section();
            }
            if action.targeting == Targeting::Direction {
                return fail(DeliveryProblem::AreaDirection);
            }
            false
        }
    };
    if action.kind == ActionKind::Attack && !homing {
        return fail(DeliveryProblem::Weapon);
    }
    Ok(())
}

/// Whether `names` holds `name`.
fn declares<'n>(mut names: impl Iterator<Item = &'n DeclaredName>, name: &str) -> bool {
    names.any(|declared| declared.as_str() == name)
}
