use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use campfire_actions::{ActionData, ActionKind, ActionsData};
use campfire_areas::AreaData;
use campfire_capabilities::{LoadNames, Release};
use campfire_common::DeclaredName;
use campfire_declare::{
    BookError, BookPlace, Section, SectionReaders, SectionTable, TypePlace, UnitTypeData,
};
use campfire_items::ItemsData;
use campfire_math::Num;
use campfire_mode::ModeData;
use campfire_orders::AiData;
use campfire_projectiles::ProjectileData;
use campfire_script::{
    ApiOwner, CtxMisuse, DataTable, EnumRecord, EnumString, Hook, MemberKind, NameKind,
    ParamProblem, ScriptApi, ScriptFacts, ScriptFunction, ScriptName, ScriptRole, Status,
    UnknownName,
};
use campfire_sim::{Capability, TickRate};
use campfire_stats::{ModeStatsData, ModifierData, ModifiersData, PassiveData, StatId};
use campfire_units::TypeStateData;
use campfire_values::{EngineTag, FilterData, MessageId, PackageIndex, PackagePath, Param, Stat};

use crate::dependent::{Dependent, DependentKind};
use crate::error::LoadError;
use crate::error::load_problem::LoadProblem;
use crate::error::place::Place;
use crate::error::script_problem::ScriptProblem;
use crate::error::way::Way;
use crate::files::mode_script::ModeScript;
use crate::mode_packages::ModePackages;
use crate::modifier_ways::ModifierWays;
use crate::package::Package;
use crate::package_view::{PackageView, ViewKind};

/// The checks at package load (08-script-api.md, Checks at package load), over a mode and every
/// package it depends on: data matches its schema (the reads checked that), every per-rank array
/// has an entry for each rank, every script is named by data and defines only hooks of its roles,
/// and every capability, `ctx` name, modifier, param, stat, filter and damage kind a script or data
/// names exists for the mode. And every homing projectile is faster than the mode's move speed cap.
#[derive(Debug)]
pub(crate) struct LoadCheck<'a> {
    packages: &'a ModePackages,
    /// The names the mode's packages declare, each kind as its owner builds it.
    names: LoadNames,
    /// The release's engine tags, which no package gives.
    engine: Vec<EngineTag>,
    /// The script API of the release, which every name a script uses must be of.
    api: &'a ScriptApi,
    /// The fastest rate the mode allows, at which a time counts the most ticks.
    rate: TickRate,
    /// Every field a script may name after `.state`: those of the mode's state, and of every
    /// modifier's and unit type's of its packages, as a script's handle has no type the load
    /// knows.
    state_fields: BTreeSet<&'a str>,
    /// The mode's sections the checks read, each as a mode that does not give it reads it.
    sections: ModeSections<'a>,
}

/// The mode's sections the checks read, each as a mode that does not give it reads it.
#[derive(Debug)]
struct ModeSections<'a> {
    stats: Cow<'a, ModeStatsData>,
}

impl<'a> ModeSections<'a> {
    fn of(data: &'a ModeData) -> Self {
        Self {
            stats: data.section_or_default(),
        }
    }
}

/// The facts one package's checks share.
#[derive(Debug)]
struct PackageNames<'a> {
    package: &'a Package,
    /// What data says of each script it names.
    scripts: BTreeMap<&'a PackagePath, ScriptUse<'a>>,
    modifiers: &'a BTreeMap<DeclaredName, ModifierData>,
    actions: &'a BTreeMap<DeclaredName, ActionData>,
}

/// What data says of a script: the roles it serves, and the params its `ctx.p` may read.
#[derive(Debug, Default)]
struct ScriptUse<'a> {
    roles: BTreeSet<ScriptRole>,
    params: BTreeSet<&'a str>,
}

impl<'a> LoadCheck<'a> {
    /// The tags a unit type or a modifier at `at` carries: none the engine's, which only the
    /// engine gives.
    fn own_tags(&self, tags: &[DeclaredName], at: &Place) -> Result<(), LoadProblem> {
        match tags
            .iter()
            .find_map(|tag| EngineTag::named(&self.engine, tag.as_str()))
        {
            Some(tag) => Err(LoadProblem::EngineTag {
                at: at.clone(),
                tag,
            }),
            None => Ok(()),
        }
    }

    /// Checks `packages` against the script API `api`; the order the stats refresh computes the
    /// mode's stats in.
    pub(crate) fn run(
        packages: &'a ModePackages,
        api: &'a ScriptApi,
    ) -> Result<Vec<StatId>, LoadError> {
        let manifest = &packages.manifest;
        let fail = |problem| LoadError::of(&manifest.header.name, problem);
        Self::sections_owned(packages, &Release::sections())?;
        let stat_order = packages
            .stat_graph()?
            .order()
            .map_err(|stats| fail(LoadProblem::StatLoop(stats)))?;
        let engine = Release::engine_tags();
        let rate = TickRate::new(manifest.tick_hz.fastest());
        let input = packages.book_input(rate, &stat_order, &engine);
        // The modules' rules of their data come first, so a script that names what a broken rule
        // leaves out shows the rule.
        let names = Release::check_rules(&input).map_err(|error| book_error(packages, error))?;
        let check = LoadCheck {
            packages,
            names,
            engine: engine.clone(),
            api,
            rate,
            state_fields: LoadCheck::state_fields(packages),
            sections: ModeSections::of(&packages.data),
        };
        check.mode().map_err(fail)?;
        check.planned_data().map_err(fail)?;
        check.loadout()?;
        for (view, dependent) in packages.packages().skip(1).zip(&packages.dependencies) {
            check
                .dependent(view, dependent)
                .map_err(|problem| LoadError::of(&dependent.package.header.name, problem))?;
        }
        Release::books(&input).map_err(|error| book_error(packages, error))?;
        // A name is the client's alone, so it is checked last: a train or a build whose other
        // problems the checks above find shows those.
        shown_named(packages.content.actions()).map_err(fail)?;
        for dependent in &packages.dependencies {
            shown_named(dependent.content.actions())
                .map_err(|problem| LoadError::of(&dependent.package.header.name, problem))?;
        }
        Ok(stat_order)
    }

    /// The mode package: its unit types' sections, its map, its modifiers and its scripts.
    fn mode(&self) -> Result<(), LoadProblem> {
        let packages = self.packages;
        let data = &packages.data;
        let content = &packages.content;
        let units = &content.units;
        self.names()?;
        for (name, unit_type) in units {
            let at = Place::UnitType(name.clone());
            Self::unit_type(unit_type, &at, content.modifiers())?;
        }
        let held = units.iter().filter_map(|(name, unit_type)| {
            Some((
                Place::UnitType(name.clone()),
                &unit_type.section::<PassiveData>()?.0,
            ))
        });
        passives_held_once(held.chain(action_passives(content.actions())))?;
        let mut names = PackageNames::new(&packages.mode, content.modifiers(), content.actions());
        let mode_params: BTreeSet<&str> = data.param_names().map(DeclaredName::as_str).collect();
        if let ModeScript::Own(path) = &packages.script {
            names.serve(path, ScriptRole::Mode, mode_params.iter().copied());
        }
        for unit_type in units.values() {
            if let Some(orders) = unit_type.section::<AiData>() {
                names.serve(&orders.ai, ScriptRole::Ai, mode_params.iter().copied());
            }
        }
        let ranks = self.slotted_ranks(units.values())?;
        let ranks = |id: &str| ranks.get(id).copied().unwrap_or(1);
        self.actions(content.actions(), &mut names)?;
        let view = packages.mode();
        let ways = packages.modifier_ways(view);
        self.modifiers(&mut names, &ways, content.actions(), ranks)?;
        self.scripts(&names)
    }

    /// The names players see of the mode, each team of its manifest, each choice and each item
    /// type: each is a message its own language's file gives, and every team has one.
    fn names(&self) -> Result<(), LoadProblem> {
        let packages = self.packages;
        let (data, mode) = (&packages.data, &packages.mode);
        message(mode, &data.name, &Place::Mode)?;
        for team in &packages.manifest.teams {
            let Some(named) = data.teams.get(&team.name) else {
                return Err(LoadProblem::UnnamedTeam(team.name.clone()));
            };
            message(mode, &named.name, &Place::Team(team.name.clone()))?;
        }
        let manifest = &packages.manifest.teams;
        if let Some(name) = data
            .teams
            .keys()
            .find(|name| !manifest.iter().any(|team| team.name == **name))
        {
            return Err(LoadProblem::Unknown {
                of: NameKind::Team,
                at: Place::Mode,
                name: name.to_string(),
            });
        }
        for (id, choice) in &data.choices {
            message(mode, &choice.name, &Place::Choice(id.clone()))?;
        }
        for (id, item) in packages.content.items() {
            message(
                mode,
                &item.name,
                &Place::Entry {
                    table: ItemsData::KEY,
                    name: id.clone(),
                },
            )?;
        }
        Ok(())
    }

    /// No entry id is held by two loadout packages, as players choose loadout entries by id.
    fn loadout(&self) -> Result<(), LoadError> {
        let mut seen = BTreeSet::new();
        for dependent in &self.packages.dependencies {
            if !matches!(dependent.kind, DependentKind::Loadout) {
                continue;
            }
            let actions = dependent.content.actions().keys();
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

    /// An avatar, loadout or rules package the mode depends on: an avatar's unit type, with every
    /// action of the package in its slots, and each action with the ranks of its kind; a
    /// loadout's actions, each with the ranks of the slot kind its choice fills; its delivery
    /// types; and its scripts, a rules package's the mode's when the mode names one of them.
    fn dependent(
        &self,
        view: PackageView<'a>,
        dependent: &'a Dependent,
    ) -> Result<(), LoadProblem> {
        let package = &dependent.package;
        let content = &dependent.content;
        let (actions, modifiers) = (content.actions(), content.modifiers());
        // Its types that stand, an avatar's summons or a loadout's: their slots hold actions of
        // the package as the avatar's do.
        let standing = || {
            content
                .units
                .values()
                .filter(|unit_type| !unit_type.delivers())
        };
        let slotted = match &dependent.kind {
            DependentKind::Avatar(avatar) => {
                let at = Place::Avatar(package.header.name.clone());
                message(package, &avatar.name, &at)?;
                named(actions)?;
                Self::unit_type(&avatar.unit, &at, modifiers)?;
                let passive = avatar.unit.section::<PassiveData>();
                let held = passive.map(|passive| (at, &passive.0));
                passives_held_once(held.into_iter().chain(action_passives(actions)))?;
                let ranks = self.slotted_ranks(standing().chain([&avatar.unit]))?;
                Some(ranks)
            }
            DependentKind::Loadout => {
                named(actions)?;
                passives_held_once(action_passives(actions))?;
                // A loadout's action loads with the loadout's ranks, so a summon of it places
                // its actions only in kinds of those ranks.
                let loadout_ranks = self.packages.data.loadout_ranks();
                let placed = self.slotted_ranks(standing())?;
                let other = placed.iter().find(|&(_, &ranks)| ranks != loadout_ranks);
                if let Some((id, _)) = other {
                    let id = DeclaredName::new(id).expect("an action's id is a declared name");
                    return Err(LoadProblem::ActionRanks(id));
                }
                None
            }
            DependentKind::Rules => None,
        };
        for (id, unit_type) in &content.units {
            Self::unit_type(unit_type, &Place::UnitType(id.clone()), modifiers)?;
        }
        let loadout_ranks = self.packages.data.loadout_ranks();
        let ranks = |id: &str| slotted.as_ref().map_or(loadout_ranks, |ranks| ranks[id]);
        let mut names = PackageNames::new(package, modifiers, actions);
        let data = &self.packages.data;
        let mode_params: BTreeSet<&str> = data.param_names().map(DeclaredName::as_str).collect();
        if let ModeScript::Rules {
            package: name,
            path,
        } = &self.packages.script
            && *name == package.header.name
        {
            names.serve(path, ScriptRole::Mode, mode_params.iter().copied());
        }
        for unit_type in content.units.values() {
            if let Some(orders) = unit_type.section::<AiData>() {
                names.serve(&orders.ai, ScriptRole::Ai, mode_params.iter().copied());
            }
        }
        self.actions(actions, &mut names)?;
        let ways = self.packages.modifier_ways(view);
        self.modifiers(&mut names, &ways, actions, ranks)?;
        self.scripts(&names)
    }

    /// The ranks `types` give each action they place, as `ModePackages::slotted_ranks` reads
    /// them; an action placed in kinds of other ranks fails.
    fn slotted_ranks<'u>(
        &self,
        types: impl IntoIterator<Item = &'u UnitTypeData>,
    ) -> Result<BTreeMap<&'u str, u8>, LoadProblem> {
        self.packages
            .slotted_ranks(types)
            .map_err(|id| LoadProblem::ActionRanks(id.clone()))
    }

    /// A package's `actions`, each with the ranks `ranks` gives it and its delivery one of the
    /// package's `units`, and the roles of their scripts in `names`: the capabilities each uses,
    /// the modifiers it names, which the modifiers' ways hold, and the params it reads.
    fn actions(
        &self,
        actions: &'a BTreeMap<DeclaredName, ActionData>,
        names: &mut PackageNames<'a>,
    ) -> Result<(), LoadProblem> {
        for (id, action) in actions {
            let at = Place::Entry {
                table: ActionsData::KEY,
                name: id.clone(),
            };
            if let Some(name) = &action.name {
                message(names.package, name, &at)?;
            }
            if let Some(script) = &action.script {
                names.serve(
                    script,
                    ScriptRole::Action,
                    action.params.keys().map(DeclaredName::as_str),
                );
            }
        }
        Ok(())
    }

    /// The package's modifiers: the capability, an aura's modifier, which the ways pass through,
    /// the params each reads, and the roles of their scripts. A modifier reads its own params,
    /// then those each of its `ways` gives, of `actions` at the ranks `ranks` gives them.
    fn modifiers(
        &self,
        names: &mut PackageNames<'a>,
        ways: &ModifierWays<'_>,
        actions: &'a BTreeMap<DeclaredName, ActionData>,
        ranks: impl Fn(&str) -> u8,
    ) -> Result<(), LoadProblem> {
        for (id, modifier) in names.modifiers {
            let at = Place::Entry {
                table: ModifiersData::KEY,
                name: id.clone(),
            };
            self.own_tags(&modifier.tags, &at)?;
            // The ways pass on through an aura's modifier, which the checks below read.
            if let Some(aura) = &modifier.aura {
                modifier_exists(names.modifiers, aura.modifier.as_str(), &at)?;
            }
            let by: Vec<&ActionData> = ways
                .actions_of(id.as_str(), actions)
                .map(|(_, action)| action)
                .collect();
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
                .chain(by.iter().flat_map(|action| action.params.keys()))
                .map(DeclaredName::as_str)
                .collect();
            let appliers = by.iter().map(|action| &action.params);
            modifier
                .check_signs(appliers)
                .map_err(|problem| LoadProblem::Modifier {
                    modifier: id.clone(),
                    problem,
                })?;
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
        let holds = |param: &Param| {
            if !number {
                return Ok(());
            }
            let numbers: Option<Vec<Num>> = param.numbers().collect();
            let numbers = numbers.ok_or(ParamProblem::Overflow)?;
            if time && matches!(param, Param::Scaling(_)) {
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
            if facts.function_pointer {
                return Err(fail(ScriptProblem::FunctionPointer));
            }
            if facts.top_level {
                return Err(fail(ScriptProblem::TopLevel));
            }
            if let Some(found) = &facts.ctx_misuse {
                return Err(misuse(found.clone()));
            }
            for function in &facts.functions {
                let Some(hook) = self.hook(function, roles).map_err(fail)? else {
                    continue;
                };
                let owner = self.api.hook(hook).and_then(|status| status.capability);
                if let Some(capability) = owner {
                    self.require(capability, &at)?;
                }
            }
            for used in &facts.ctx_names {
                let member = self.api.member(ApiOwner::Ctx, &used.name).filter(|member| {
                    member.spec.kind == used.kind
                        && roles.iter().any(|&role| member.spec.roles.contains(role))
                });
                let Some(member) = member else {
                    return Err(fail(ScriptProblem::UnknownCtx(used.name.clone())));
                };
                if member.status == Status::Planned {
                    return Err(fail(ScriptProblem::Planned(used.name.clone())));
                }
                if let Some(capability) = member.spec.capability {
                    self.require(capability, &at)?;
                }
            }
            for used in &facts.members {
                if let Some(capability) = self.member(facts, &used.name, used.kind).map_err(fail)? {
                    self.require(capability, &at)?;
                }
            }
            self.enums(facts).map_err(fail)?;
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
                self.script_name(named, names, path)?;
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

    /// What a script with `facts` gives the engine enums: a member, never a string literal, to an
    /// argument that takes one, and of each `Module::name` path, a member or a function of the
    /// enum, whose module the compile checked.
    fn enums(&self, facts: &ScriptFacts) -> Result<(), ScriptProblem> {
        if let Some(EnumString { call, engine_enum }) = facts.enum_strings.first() {
            return Err(ScriptProblem::EnumString {
                call: call.clone(),
                takes: *engine_enum,
            });
        }
        for path in &facts.enum_paths {
            let record = self
                .api
                .enum_named(&path.module)
                .expect("the compile refuses a module that is no engine enum's");
            let known = match path.kind {
                MemberKind::Value => record.has(&path.name),
                _ => EnumRecord::FUNCTIONS.contains(&path.name.as_str()),
            };
            if !known {
                return Err(ScriptProblem::UnknownEnumMember {
                    path: path.to_string(),
                    of: record.engine_enum,
                });
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
            let modifiers =
                (content.modifiers().values()).flat_map(|modifier| modifier.state.keys());
            let avatar = match view.kind {
                ViewKind::Avatar(avatar) => Some(&avatar.unit),
                ViewKind::Mode | ViewKind::Loadout | ViewKind::Rules => None,
            };
            let units = content.units.values().chain(avatar);
            let units = units.filter_map(|unit| unit.section::<TypeStateData>());
            let units = units.flat_map(|state| state.0.keys());
            fields.extend(modifiers.chain(units).map(DeclaredName::as_str));
        }
        fields
    }

    /// The hook `function` defines, if its name is a hook's: one of `roles` that runs, with the
    /// hook's parameters, and `ctx` first. A name with a hook's prefix that names none fails.
    fn hook(
        &self,
        function: &ScriptFunction,
        roles: &BTreeSet<ScriptRole>,
    ) -> Result<Option<Hook>, ScriptProblem> {
        let name = &function.name;
        let Some(hook) = Hook::named(name) else {
            if Hook::PREFIXES.iter().any(|prefix| name.starts_with(prefix)) {
                return Err(ScriptProblem::UnknownHook(name.clone()));
            }
            return Ok(None);
        };
        if !roles.contains(&hook.role()) || hook.params() != function.params {
            return Err(ScriptProblem::UnknownHook(name.clone()));
        }
        let planned = self
            .api
            .hook(hook)
            .is_some_and(|held| held.status == Status::Planned);
        if planned {
            return Err(ScriptProblem::Planned(name.clone()));
        }
        if !function.ctx_first {
            return Err(ScriptProblem::CtxMisuse(CtxMisuse::HookParam {
                function: name.clone(),
            }));
        }
        Ok(Some(hook))
    }

    /// A script may read or call `name`, of `kind`, on a value other than `ctx`: a handle's field
    /// or method that runs, one the engine has of its own, a key of the script's object maps or
    /// one of its functions. A name only a planned handle member has is planned. A name whose
    /// every form that runs is of a capability the mode does not declare gives the first such
    /// capability, which the match binds no form of.
    fn member(
        &self,
        facts: &ScriptFacts,
        name: &str,
        kind: MemberKind,
    ) -> Result<Option<Capability>, ScriptProblem> {
        let own = self.api.builtin(name, kind)
            || match kind {
                MemberKind::Field => facts.map_keys.iter().any(|key| key == name),
                _ => facts.functions.iter().any(|function| function.name == name),
            };
        if own {
            return Ok(None);
        }
        let mut problem = ScriptProblem::UnknownMember(name.to_owned());
        let mut undeclared = None;
        let handles = self.api.members().iter().filter(|member| {
            member.spec.owner != ApiOwner::Ctx
                && member.spec.name == name
                && member.spec.kind == kind
        });
        for member in handles {
            if member.status == Status::Planned {
                problem = ScriptProblem::Planned(name.to_owned());
                continue;
            }
            match member.spec.capability {
                Some(capability) if !self.packages.manifest.capabilities.contains(capability) => {
                    undeclared.get_or_insert(capability);
                }
                _ => return Ok(None),
            }
        }
        undeclared.map(Some).ok_or(problem)
    }

    /// A name the script at `path` gives an argument of a name kind is one of its kind that the
    /// match has: a modifier, an ability or a message one of the script's package's, as `package`
    /// holds them; a param one data gives the script; a unit type one of the mode's scope.
    fn script_name(
        &self,
        named: &ScriptName,
        package: &PackageNames<'_>,
        path: &PackagePath,
    ) -> Result<(), LoadProblem> {
        let at = Place::Script(path.clone());
        let unknown = || LoadProblem::Unknown {
            of: named.kind,
            at: at.clone(),
            name: named.name.clone(),
        };
        let packages = self.packages;
        let data = &packages.data;
        let name = named.name.as_str();
        let known = match self.names.has(named.kind, name) {
            Some(known) => known,
            None => match named.kind {
                NameKind::Modifier => return modifier_exists(package.modifiers, name, &at),
                NameKind::Ability => package.actions.contains_key(name),
                NameKind::Message => package.package.text.gives(name),
                NameKind::Param => package
                    .scripts
                    .get(path)
                    .is_some_and(|script| script.params.contains(name)),
                NameKind::Filter => return self.filter_text(name, &at),
                NameKind::Stat => {
                    let stat = Stat::named(name).ok_or_else(unknown)?;
                    return self.stats_declared([&stat], &at);
                }
                NameKind::Choice => declares(data.choices.keys(), name),
                NameKind::Item => packages.content.items().contains_key(name),
                kind => unreachable!("the load's names hold every {kind}"),
            },
        };
        if known { Ok(()) } else { Err(unknown()) }
    }

    /// Each section of the packages' data is of a capability the mode declares, as a match builds
    /// the books of a section and checks its rules only for the module that registers it: of the
    /// mode's data, of its map, and of each package's content and unit types.
    fn sections_owned(packages: &ModePackages, readers: &SectionReaders) -> Result<(), LoadError> {
        let undeclared =
            |(key, capability), at: fn(&'static str) -> Place| LoadProblem::Undeclared {
                capability,
                at: at(key),
            };
        let mode = Self::undeclared(
            packages,
            readers,
            SectionTable::Mode,
            packages.data.sections.keys(),
        );
        let mode = mode.map(|found| undeclared(found, Place::Section));
        let map = || {
            let map = Self::undeclared(
                packages,
                readers,
                SectionTable::Map,
                packages.map.sections().keys(),
            );
            map.map(|found| undeclared(found, Place::MapSection))
        };
        if let Some(problem) = mode.or_else(map) {
            return Err(LoadError::of(&packages.manifest.header.name, problem));
        }
        for view in packages.packages() {
            let content = Self::undeclared(
                packages,
                readers,
                SectionTable::Package,
                view.content.sections.keys(),
            );
            let content = content.map(|found| undeclared(found, Place::Section));
            let avatar = match view.kind {
                ViewKind::Avatar(avatar) => Some((
                    Place::Avatar(view.package.header.name.clone()),
                    &avatar.unit,
                )),
                ViewKind::Mode | ViewKind::Loadout | ViewKind::Rules => None,
            };
            let units = view.content.units.iter();
            let mut units = units.map(|(name, unit)| (Place::UnitType(name.clone()), unit));
            let unit = || {
                units.by_ref().chain(avatar).find_map(|(at, unit)| {
                    let (_, capability) = Self::undeclared(
                        packages,
                        readers,
                        SectionTable::UnitType,
                        unit.sections().keys(),
                    )?;
                    Some(LoadProblem::Undeclared { capability, at })
                })
            };
            if let Some(problem) = content.or_else(unit) {
                return Err(LoadError::of(&view.package.header.name, problem));
            }
        }
        Ok(())
    }

    /// The first of `keys`, sections of `table`, whose module is of a capability the mode does not
    /// declare, with that capability.
    fn undeclared(
        packages: &ModePackages,
        readers: &SectionReaders,
        table: SectionTable,
        keys: impl IntoIterator<Item = &'static str>,
    ) -> Option<(&'static str, Capability)> {
        let declared = &packages.manifest.capabilities;
        keys.into_iter().find_map(|key| {
            let owner = readers.owner(table, key);
            let capability = owner.filter(|&capability| !declared.contains(capability))?;
            Some((key, capability))
        })
    }

    /// A unit type at `at`, of a package of `modifiers`: an area's `inside` modifiers of
    /// `modifiers`.
    fn unit_type(
        unit_type: &UnitTypeData,
        at: &Place,
        modifiers: &BTreeMap<DeclaredName, ModifierData>,
    ) -> Result<(), LoadProblem> {
        if let Some(area) = unit_type.section::<AreaData>() {
            // The modifiers' ways, which the modifiers' checks read, pass on through an area's
            // `inside` modifiers.
            for modifier in area.inside.modifiers() {
                modifier_exists(modifiers, modifier.as_str(), at)?;
            }
        }
        Ok(())
    }

    /// No data gives a field the script API plans: the mode's `state_version`, or a projectile
    /// type's `gravity` in any package. An action's planned effects fail the action's own checks.
    fn planned_data(&self) -> Result<(), LoadProblem> {
        let planned = |table: DataTable, field: &str| {
            self.api.data().iter().any(|held| {
                held.table == table && held.name == field && held.status == Status::Planned
            })
        };
        let packages = self.packages;
        if packages.data.state_version.is_some() && planned(DataTable::Mode, "state_version") {
            return Err(LoadProblem::Planned {
                field: "state_version",
                at: Place::Mode,
            });
        }
        let units = packages
            .packages()
            .flat_map(|view| view.content.units.iter());
        for (name, unit_type) in units {
            let falls = (unit_type.section::<ProjectileData>())
                .is_some_and(|projectile| projectile.gravity.is_some());
            if falls && planned(DataTable::Projectile, "gravity") {
                return Err(LoadProblem::Planned {
                    field: "gravity",
                    at: Place::UnitType(name.clone()),
                });
            }
        }
        Ok(())
    }

    /// Every stat in `stats` is one the mode declares.
    fn stats_declared<'s>(
        &self,
        stats: impl IntoIterator<Item = &'s Stat>,
        at: &Place,
    ) -> Result<(), LoadProblem> {
        let declared = &self.sections.stats.0;
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
            .all(|tag| self.names.has(NameKind::Tag, tag.name.as_str()) == Some(true))
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

/// The load error of what building `packages`' books refused, at the fastest rate the mode
/// allows.
fn book_error(packages: &ModePackages, error: BookError) -> LoadError {
    let (package, problem) = match error {
        BookError::Unknown {
            package,
            at,
            name: UnknownName { of, name },
        } => (
            package,
            LoadProblem::Unknown {
                at: book_place(packages, package, at),
                name,
                of,
            },
        ),
        BookError::Undeclared {
            package,
            at,
            capability,
        } => (
            package,
            LoadProblem::Undeclared {
                capability,
                at: book_place(packages, package, at),
            },
        ),
        BookError::Script { package, path } => (
            package,
            LoadProblem::Script {
                path,
                problem: ScriptProblem::Missing,
            },
        ),
        BookError::Rule {
            package,
            at,
            problem,
        } => (
            package,
            LoadProblem::Rule {
                at: book_place(packages, package, at),
                problem,
            },
        ),
    };
    let view = packages.packages().nth(usize::from(package));
    LoadError::of(
        &view.expect("the books name a package").package.header.name,
        problem,
    )
}

/// The place of `at`, of the package at `package`, as a load error names it.
fn book_place(packages: &ModePackages, package: PackageIndex, at: BookPlace) -> Place {
    match at {
        BookPlace::UnitType(TypePlace::Avatar) => {
            let view = packages.packages().nth(usize::from(package));
            let package = &view.expect("the books name a package").package;
            Place::Avatar(package.header.name.clone())
        }
        BookPlace::UnitType(TypePlace::Declared(name)) => Place::UnitType(name),
        BookPlace::Entry { table, name } => Place::Entry { table, name },
        BookPlace::Section(key) => Place::Section(key),
        BookPlace::MapSection(key) => Place::MapSection(key),
        BookPlace::Map => Place::Map,
        BookPlace::Teams => Place::Mode,
        BookPlace::Types => Place::UnitTypes,
    }
}

impl<'a> PackageNames<'a> {
    const fn new(
        package: &'a Package,
        modifiers: &'a BTreeMap<DeclaredName, ModifierData>,
        actions: &'a BTreeMap<DeclaredName, ActionData>,
    ) -> Self {
        PackageNames {
            package,
            scripts: BTreeMap::new(),
            modifiers,
            actions,
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

/// The passive of each of `actions`, by its place.
fn action_passives(
    actions: &BTreeMap<DeclaredName, ActionData>,
) -> impl Iterator<Item = (Place, &DeclaredName)> {
    actions.iter().filter_map(|(id, action)| {
        Some((
            Place::Entry {
                table: ActionsData::KEY,
                name: id.clone(),
            },
            action.passive_modifier.as_ref()?,
        ))
    })
}

/// Whether `package`'s own language's file gives the message `id`, which data at `at` names.
fn message(package: &Package, id: &MessageId, at: &Place) -> Result<(), LoadProblem> {
    if package.text.gives(id.as_str()) {
        return Ok(());
    }
    Err(LoadProblem::Unknown {
        of: NameKind::Message,
        at: at.clone(),
        name: id.to_string(),
    })
}

/// Whether each of `actions`, a player's, has a name.
fn named(actions: &BTreeMap<DeclaredName, ActionData>) -> Result<(), LoadProblem> {
    match actions.iter().find(|(_, action)| action.name.is_none()) {
        Some((id, _)) => Err(LoadProblem::UnnamedAction(id.clone())),
        None => Ok(()),
    }
}

/// Whether each train and build of `actions` has a name, which a client shows on its
/// producer's button.
fn shown_named(actions: &BTreeMap<DeclaredName, ActionData>) -> Result<(), LoadProblem> {
    let shown = |action: &ActionData| matches!(action.kind, ActionKind::Train | ActionKind::Build);
    match actions
        .iter()
        .find(|(_, action)| shown(action) && action.name.is_none())
    {
        Some((id, _)) => Err(LoadProblem::UnnamedAction(id.clone())),
        None => Ok(()),
    }
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

/// Whether `names` holds `name`.
fn declares<'n>(mut names: impl Iterator<Item = &'n DeclaredName>, name: &str) -> bool {
    names.any(|declared| declared.as_str() == name)
}
