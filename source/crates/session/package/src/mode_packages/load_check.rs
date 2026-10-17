use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::slice;

use campfire_actions::{
    ActionData, ActionKind, ActionRange, ActionsData, DamageFields, DeliveryData, DoesData,
    EffectData, EffectTo, HealFields, KindData, KindError, LaunchFields, ListHook, ModifierFields,
    MoveData, PurgeFields, RangeField, RestoreFields, SlotKinds, SpawnFields, Targeting,
    TypeSlotsData, XpFields,
};
use campfire_areas::AreaData;
use campfire_capabilities::Release;
use campfire_combat::{CombatData, ModeCombatData};
use campfire_common::DeclaredName;
use campfire_declare::{
    BookError, BookPlace, Section, SectionReaders, SectionTable, TypePlace, UnitTypeData,
};
use campfire_geometry::Metric;
use campfire_items::ItemsData;
use campfire_math::Num;
use campfire_mode::ModeData;
use campfire_navigation::ModeNavigationData;
use campfire_orders::AiData;
use campfire_players::ResourcesData;
use campfire_production::{ModeSupplyData, ProductionData, SupplyData};
use campfire_projectiles::{ProjectileData, ProjectileHits};
use campfire_script::{
    ApiOwner, CtxMisuse, DataTable, EnumRecord, EnumString, Hook, MemberKind, NameKind,
    ParamProblem, ScriptApi, ScriptFacts, ScriptFunction, ScriptName, ScriptRole, Status,
    UnknownName,
};
use campfire_sim::{Capability, TickRate};
use campfire_stats::{
    ModePoolsData, ModeStatsData, ModifierData, ModifiersData, PassiveData, StatId, StatsData,
    TypePoolsData,
};
use campfire_units::{CollisionData, TypeParamsData, TypeStateData, TypeTagsData};
use campfire_values::{EngineTag, FilterData, MessageId, PackageIndex, PackagePath, Param, Stat};
use campfire_vision::VisionData;

use crate::dependent::{Dependent, DependentKind};
use crate::error::LoadError;
use crate::error::box_problem::BoxProblem;
use crate::error::build_problem::BuildProblem;
use crate::error::delivery_problem::DeliveryProblem;
use crate::error::effect_problem::EffectProblem;
use crate::error::gather_problem::GatherProblem;
use crate::error::load_problem::LoadProblem;
use crate::error::place::Place;
use crate::error::script_problem::ScriptProblem;
use crate::error::use_problem::UseProblem;
use crate::error::way::Way;
use crate::files::mode_script::ModeScript;
use crate::mode_packages::ModePackages;
use crate::mode_packages::name_tables::{NameTables, TableKind};
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
    /// The names the mode's packages declare, each kind in its table.
    names: NameTables,
    /// The release's engine tags, which no package gives.
    engine: Vec<EngineTag>,
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
    /// The mode's sections the checks read, each as a mode that does not give it reads it.
    sections: ModeSections<'a>,
    /// The sections of every module of the release, each with the capability of its module.
    readers: SectionReaders,
}

/// The mode's sections the checks read, each as a mode that does not give it reads it.
#[derive(Debug)]
struct ModeSections<'a> {
    combat: Cow<'a, ModeCombatData>,
    navigation: Cow<'a, ModeNavigationData>,
    slots: Cow<'a, SlotKinds>,
    stats: Cow<'a, ModeStatsData>,
    pools: Cow<'a, ModePoolsData>,
    resources: Cow<'a, ResourcesData>,
}

impl<'a> ModeSections<'a> {
    fn of(data: &'a ModeData) -> Self {
        Self {
            combat: data.section_or_default(),
            navigation: data.section_or_default(),
            slots: data.section_or_default(),
            stats: data.section_or_default(),
            pools: data.section_or_default(),
            resources: data.section_or_default(),
        }
    }
}

/// What the action whose effects a check reads may name: its package's unit types, those it
/// delivers and those it spawns.
#[derive(Debug, Clone, Copy)]
struct EffectScope<'u> {
    units: &'u BTreeMap<DeclaredName, UnitTypeData>,
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
        let engine = Release::engine_tags();
        let names = NameTables::of(packages);
        let check = LoadCheck {
            packages,
            names: names.map_err(|error| book_error(packages, error))?,
            engine,
            cap: manifest.max_move_speed.get(),
            api,
            rate: TickRate::new(manifest.tick_hz.fastest()),
            state_fields: LoadCheck::state_fields(packages),
            sections: ModeSections::of(&packages.data),
            readers: Release::sections(),
        };
        check.sections_owned()?;
        check.mode().map_err(fail)?;
        check.planned_data().map_err(fail)?;
        check.loadout()?;
        for (view, dependent) in packages.packages().skip(1).zip(&packages.dependencies) {
            check
                .dependent(view, dependent)
                .map_err(|problem| LoadError::of(&dependent.package.header.name, problem))?;
        }
        let stat_order = packages
            .stat_graph()?
            .order()
            .map_err(|stats| fail(LoadProblem::StatLoop(stats)))?;
        let engine_tags = Release::engine_tags();
        let input = packages.book_input(check.rate, &stat_order, &engine_tags);
        Release::check_books(&input).map_err(|error| book_error(packages, error))?;
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
        self.damage_kinds()?;
        self.pools_and_resources()?;
        for (name, unit_type) in units {
            let at = Place::UnitType(name.clone());
            self.unit_type(unit_type, &at, content.actions(), content.modifiers())?;
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
        self.actions(content.actions(), units, &mut names)?;
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
                self.unit_type(&avatar.unit, &at, actions, modifiers)?;
                let passive = avatar.unit.section::<PassiveData>();
                let held = passive.map(|passive| (at, &passive.0));
                passives_held_once(held.into_iter().chain(action_passives(actions)))?;
                let ranks = self.slotted_ranks(standing().chain([&avatar.unit]))?;
                let unslotted = actions.keys().find(|id| !ranks.contains_key(id.as_str()));
                if let Some(id) = unslotted {
                    return Err(LoadProblem::Unslotted(id.clone()));
                }
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
            self.unit_type(unit_type, &Place::UnitType(id.clone()), actions, modifiers)?;
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
        self.actions(actions, &content.units, &mut names)?;
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
        units: &BTreeMap<DeclaredName, UnitTypeData>,
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
            self.kind(id, action, units)?;
            weapon_fields_hold(id, action, units)?;
            requires(id, action, units)?;
            self.cost_names(id, action)?;
            let scope = EffectScope { units };
            self.effects(id, action, scope)?;
            if let Some(delivery) = &action.delivery {
                let capability = match delivery {
                    DeliveryData::Projectile { .. } => Capability::Projectiles,
                    DeliveryData::Area { .. } => Capability::Areas,
                };
                self.require(capability, &at)?;
                let hooks_hit = action
                    .script
                    .as_ref()
                    .and_then(|path| names.package.script(path))
                    .is_some_and(|script| script.defines(Hook::OnHit));
                delivery_holds(id, action, delivery, units, hooks_hit)?;
            }
            // The modifiers' ways, which the modifiers' checks read, hold the modifiers the
            // action names: one the package does not have is the action's problem, not a missing
            // way of another's.
            for modifier in action.modifiers() {
                modifier_exists(names.modifiers, modifier.as_str(), &at)?;
            }
            for name in action.param_refs() {
                if !action.params.contains_key(name) {
                    return Err(LoadProblem::Unknown {
                        of: NameKind::Param,
                        at,
                        name: name.to_string(),
                    });
                }
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
        let table = |kind| self.names.has(kind, name);
        let known = match named.kind {
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
            NameKind::Armor => self.sections.combat.armors.contains_key(name),
            NameKind::Cost => self.names.cost_target(name).is_some(),
            NameKind::MarkerTag => {
                let spawns = declares(self.sections.navigation.spawn_tags.iter(), name);
                spawns
                    || packages
                        .map
                        .markers
                        .iter()
                        .any(|marker| declares(marker.tags.iter(), name))
            }
            NameKind::Resource => table(TableKind::Resource),
            NameKind::Pool => table(TableKind::Pool),
            NameKind::DamageKind => table(TableKind::DamageKind),
            NameKind::Track => table(TableKind::Track),
            NameKind::SlotKind => table(TableKind::SlotKind),
            NameKind::Layer => table(TableKind::Layer),
            NameKind::Tag => table(TableKind::Tag),
            NameKind::Team => table(TableKind::Team),
            NameKind::Path => table(TableKind::Path),
            NameKind::UnitType => table(TableKind::UnitType),
        };
        if known { Ok(()) } else { Err(unknown()) }
    }

    /// The mode's damage kinds: with `combat`, at least one.
    fn damage_kinds(&self) -> Result<(), LoadProblem> {
        let kinds = &self.sections.combat.damage_kinds;
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

    /// A `gather` of `production` and `navigation`, whose worker walks, `id`: aimed at a unit.
    fn gather(&self, id: &DeclaredName, action: &ActionData) -> Result<(), LoadProblem> {
        let at = Place::Entry {
            table: ActionsData::KEY,
            name: id.clone(),
        };
        self.require(Capability::Production, &at)?;
        self.require(Capability::Navigation, &at)?;
        if !matches!(action.targeting, Targeting::Unit(_)) {
            return Err(LoadProblem::Gather(GatherProblem::Aims(id.to_owned())));
        }
        Ok(())
    }

    /// A `build` of `production` and `navigation`, which tests its box for room and walks its
    /// worker, `id`: aimed at a point. That it builds a unit type of its package that stands, the
    /// book build holds.
    fn build(&self, id: &DeclaredName, action: &ActionData) -> Result<(), LoadProblem> {
        let at = Place::Entry {
            table: ActionsData::KEY,
            name: id.clone(),
        };
        self.require(Capability::Production, &at)?;
        self.require(Capability::Navigation, &at)?;
        if action.targeting != Targeting::Point {
            return Err(LoadProblem::Build(BuildProblem::Aims(id.to_owned())));
        }
        Ok(())
    }

    /// An action `id` of a kind the release runs, with the capability of its kind and the fields
    /// the table of action fields lets its kind take: a `cast` of `abilities`; an `attack` of
    /// `combat`, aimed at a unit, of a range in meters; a `use` of `interaction` and `abilities`,
    /// whose casts run it, aimed at a unit;
    /// a `train` of `production`, aimed at nothing, of a unit type of its package, `units`, that
    /// walks, when the package has it.
    fn kind(
        &self,
        id: &DeclaredName,
        action: &ActionData,
        units: &BTreeMap<DeclaredName, UnitTypeData>,
    ) -> Result<(), LoadProblem> {
        let at = Place::Entry {
            table: ActionsData::KEY,
            name: id.clone(),
        };
        let kind = match KindData::of(action) {
            Ok(kind) => Some(kind),
            Err(KindError::NotRun(_)) => None,
            Err(KindError::Field(field)) => {
                return Err(LoadProblem::KindField {
                    action: id.to_owned(),
                    field,
                });
            }
        };
        if action.clamp_to_range && action.targeting != Targeting::Point {
            return Err(LoadProblem::ClampAims(id.to_owned()));
        }
        if action.hold.is_some() && action.toggle.is_none() && action.channel.is_none() {
            return Err(LoadProblem::HoldAlone(id.to_owned()));
        }
        let Some(kind) = kind else {
            return Err(LoadProblem::KindNotRun {
                action: id.clone(),
                kind: action.kind,
            });
        };
        match kind {
            KindData::Cast => {
                self.require(Capability::Abilities, &at)?;
            }
            KindData::Use => {
                self.require(Capability::Interaction, &at)?;
                self.require(Capability::Abilities, &at)?;
                if !matches!(action.targeting, Targeting::Unit(_)) {
                    return Err(LoadProblem::Use(UseProblem::Aims(id.to_owned())));
                }
            }
            KindData::Train { unit_type: name } => {
                self.require(Capability::Production, &at)?;
                if action.targeting != Targeting::None {
                    return Err(LoadProblem::TrainAims(id.to_owned()));
                }
                if let Some(unit_type) = units.get(name) {
                    if unit_type.delivers() {
                        return Err(LoadProblem::Delivery(DeliveryProblem::Trained(
                            id.to_owned(),
                        )));
                    }
                    if !unit_type
                        .section::<StatsData>()
                        .is_some_and(StatsData::walks)
                    {
                        return Err(LoadProblem::TrainStands(id.to_owned()));
                    }
                }
            }
            KindData::Build { .. } => self.build(id, action)?,
            KindData::Gather { .. } => self.gather(id, action)?,
            KindData::Attack { .. } => {
                self.require(Capability::Combat, &at)?;
                let global = action.range.as_ref().is_some_and(|range| {
                    range
                        .values()
                        .contains(&RangeField::Range(ActionRange::Global))
                });
                if !matches!(action.targeting, Targeting::Unit(_)) {
                    return Err(LoadProblem::AttackAims(id.to_owned()));
                }
                if global {
                    return Err(LoadProblem::GlobalAttack(id.to_owned()));
                }
            }
        }
        Ok(())
    }

    /// The effect lists of `action`, whose id is `id`, of a package of `units`: a hit's or an
    /// end's list of an action that delivers, or of an attack, which reaches its target at once;
    /// and each list as `effect_list` checks it.
    fn effects(
        &self,
        id: &DeclaredName,
        action: &ActionData,
        scope: EffectScope<'_>,
    ) -> Result<(), LoadProblem> {
        for (list, effects) in action.effect_lists() {
            let delivers = action.delivery.is_some() || action.kind == ActionKind::Attack;
            if !effects.is_empty() && list != ListHook::OnResolve && !delivers {
                return Err(LoadProblem::Effect {
                    action: id.clone(),
                    list,
                    problem: EffectProblem::NoDelivery,
                });
            }
            let reaches = match list {
                ListHook::OnResolve => matches!(action.targeting, Targeting::Unit(_)),
                ListHook::OnHit => true,
                ListHook::OnEnd => false,
            };
            self.effect_list(id, action, scope, list, effects, reaches)?;
        }
        Ok(())
    }

    /// A forced move `moves`, at `at`, in a list that reaches a unit when `reaches`: its other
    /// unit one the list reaches; `fail` makes the problem of the effect's list.
    fn forced_move(
        &self,
        moves: &MoveData,
        reaches: bool,
        at: &Place,
        fail: impl Fn(EffectProblem) -> LoadProblem,
    ) -> Result<(), LoadProblem> {
        self.require(Capability::Navigation, at)?;
        let other = match moves {
            MoveData::Dash { to, .. } => to,
            MoveData::KnockBack { from, .. } => from,
        };
        if *other == EffectTo::Reached && !reaches {
            return Err(fail(EffectProblem::NoUnit));
        }
        Ok(())
    }

    /// The list `effects` of `action`, whose id is `id`, which runs before `list` and reaches a
    /// unit when `reaches`: each effect of a capability the mode declares, to a unit the list
    /// reaches; a launch of an area type of `units`, whose `on_hit` reaches a unit and whose
    /// `on_end` does not. The other names they give, the rules of their numbers and durations,
    /// and the effects the release does not run, the book build holds.
    fn effect_list(
        &self,
        id: &DeclaredName,
        action: &ActionData,
        scope: EffectScope<'_>,
        list: ListHook,
        effects: &[EffectData],
        reaches: bool,
    ) -> Result<(), LoadProblem> {
        let at = Place::Entry {
            table: ActionsData::KEY,
            name: id.clone(),
        };
        let fail = |problem| LoadProblem::Effect {
            action: id.clone(),
            list,
            problem,
        };
        // An `on_resolve` aimed at a point reaches no unit, and a spawn there takes the point.
        let aims_point = list == ListHook::OnResolve
            && matches!(action.targeting, Targeting::Point | Targeting::Direction);
        for effect in effects {
            let placed = aims_point && matches!(effect.does, DoesData::Spawn(SpawnFields { .. }));
            if effect.to == EffectTo::Reached && !reaches && !placed {
                return Err(fail(EffectProblem::NoUnit));
            }
            let unknown = |of, name: &DeclaredName| LoadProblem::Unknown {
                of,
                at: at.clone(),
                name: name.to_string(),
            };
            match &effect.does {
                DoesData::Planned(_) => {}
                DoesData::Damage(DamageFields { .. })
                | DoesData::Heal(HealFields { .. })
                | DoesData::Restore(RestoreFields { .. }) => {
                    self.require(Capability::Combat, &at)?;
                }
                DoesData::Modifier(ModifierFields { .. }) => {
                    self.require(Capability::Stats, &at)?;
                }
                DoesData::Xp(XpFields { .. }) => self.require(Capability::Progression, &at)?,
                DoesData::Purge(PurgeFields { tag }) => {
                    self.require(Capability::Stats, &at)?;
                    self.own_tags(slice::from_ref(tag), &at)?;
                }
                DoesData::Move(moves) => self.forced_move(moves, reaches, &at, fail)?,
                DoesData::Launch(LaunchFields {
                    area,
                    on_hit,
                    on_end,
                }) => {
                    self.require(Capability::Areas, &at)?;
                    let unit_type = scope
                        .units
                        .get(area)
                        .ok_or_else(|| unknown(NameKind::UnitType, area))?;
                    if unit_type.section::<AreaData>().is_none() {
                        return Err(LoadProblem::Delivery(DeliveryProblem::WrongSection {
                            action: id.clone(),
                            unit_type: area.clone(),
                        }));
                    }
                    self.effect_list(id, action, scope, ListHook::OnHit, on_hit, true)?;
                    self.effect_list(id, action, scope, ListHook::OnEnd, on_end, false)?;
                }
                DoesData::Spawn(SpawnFields { unit_type, .. }) => {
                    scope.spawn(unit_type, &at, fail)?;
                }
            }
        }
        Ok(())
    }

    /// Each section of the packages' data is of a capability the mode declares, as a match builds
    /// the books of a section and checks its rules only for the module that registers it: of the
    /// mode's data, of its map, and of each package's content and unit types.
    fn sections_owned(&self) -> Result<(), LoadError> {
        let packages = self.packages;
        let undeclared =
            |(key, capability), at: fn(&'static str) -> Place| LoadProblem::Undeclared {
                capability,
                at: at(key),
            };
        let mode = self.undeclared(SectionTable::Mode, packages.data.sections.keys());
        let mode = mode.map(|found| undeclared(found, Place::Section));
        let map = || {
            let map = self.undeclared(SectionTable::Map, packages.map.sections().keys());
            map.map(|found| undeclared(found, Place::MapSection))
        };
        if let Some(problem) = mode.or_else(map) {
            return Err(LoadError::of(&packages.manifest.header.name, problem));
        }
        for view in packages.packages() {
            let content = self.undeclared(SectionTable::Package, view.content.sections.keys());
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
                    let (_, capability) =
                        self.undeclared(SectionTable::UnitType, unit.sections().keys())?;
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
        &self,
        table: SectionTable,
        keys: impl IntoIterator<Item = &'static str>,
    ) -> Option<(&'static str, Capability)> {
        let declared = &self.packages.manifest.capabilities;
        keys.into_iter().find_map(|key| {
            let owner = self.readers.owner(table, key);
            let capability = owner.filter(|&capability| !declared.contains(capability))?;
            Some((key, capability))
        })
    }

    /// The mode's pools and player resources: no pool named as a resource, a pool only with
    /// `stats`; and with `combat`, a life pool.
    fn pools_and_resources(&self) -> Result<(), LoadProblem> {
        let data = &self.sections;
        if let Some(name) = (data.resources.0.iter()).find(|name| data.pools.0.contains_key(*name))
        {
            return Err(LoadProblem::Repeated {
                at: Place::Section(ResourcesData::KEY),
                name: name.to_string(),
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

    /// A box body only on a planar map.
    fn body_box(&self, unit_type: &UnitTypeData, at: &Place) -> Result<(), LoadProblem> {
        let boxed = unit_type
            .section::<CollisionData>()
            .is_some_and(|collision| collision.form.is_box());
        if boxed && self.packages.map.metric() == Metric::Spatial {
            let at = at.clone();
            return Err(LoadProblem::BoxBody {
                at,
                problem: BoxProblem::Spatial,
            });
        }
        Ok(())
    }

    /// A `supply` of `unit_type`, at `at`, needs the mode's `[supply]`.
    fn sections(&self, unit_type: &UnitTypeData, at: &Place) -> Result<(), LoadProblem> {
        let counted = self.packages.data.section::<ModeSupplyData>().is_some();
        if unit_type.section::<SupplyData>().is_some() && !counted {
            return Err(LoadProblem::SupplyUncounted(at.clone()));
        }
        Ok(())
    }

    /// A unit type at `at`, of a package of `actions` and `modifiers`: each capability its
    /// sections use declared, its pools as `unit_pools` takes them, a box body only where
    /// `body_box` allows it; its slots of kinds the mode declares, each holding actions of
    /// `actions`, none twice, a train only on a type with a queue; its inventory as `inventory`
    /// takes it; and a projectile or an area type a delivery type alone: a homing projectile
    /// faster than the cap, one that hits nothing neither homing, stopping, hitting once a cast
    /// nor wide, but a curve, whose width meets its target, and an area's `inside` modifiers of
    /// `modifiers`.
    fn unit_type(
        &self,
        unit_type: &UnitTypeData,
        at: &Place,
        actions: &BTreeMap<DeclaredName, ActionData>,
        modifiers: &BTreeMap<DeclaredName, ModifierData>,
    ) -> Result<(), LoadProblem> {
        self.sections(unit_type, at)?;
        self.own_tags(&unit_type.section_or_default::<TypeTagsData>().0, at)?;
        self.body_box(unit_type, at)?;
        if unit_type.delivers() && !delivery_only(unit_type) {
            return Err(LoadProblem::Delivery(DeliveryProblem::NotDelivery(
                at.clone(),
            )));
        }
        if let Some(area) = unit_type.section::<AreaData>() {
            // The modifiers' ways, which the modifiers' checks read, pass on through an area's
            // `inside` modifiers.
            for modifier in area.inside.modifiers() {
                modifier_exists(modifiers, modifier.as_str(), at)?;
            }
        }
        if let Some(projectile) = unit_type.section::<ProjectileData>() {
            if projectile.homing && projectile.speed <= self.cap {
                return Err(LoadProblem::Delivery(DeliveryProblem::NotFaster(
                    at.clone(),
                )));
            }
            let hits_aside = projectile.homing
                || projectile.stop_on_hit
                || projectile.once_per_cast
                || (projectile.width > Num::ZERO && projectile.curve.is_none());
            if projectile.hits == ProjectileHits::Nothing && hits_aside {
                return Err(LoadProblem::Delivery(DeliveryProblem::HitsNothing(
                    at.clone(),
                )));
            }
        }
        self.unit_pools(
            &unit_type.section_or_default::<TypePoolsData>().0,
            unit_type.section::<CombatData>().is_some(),
            at,
        )?;
        // The slotted ranks, which the actions' checks read, hold only the slots of a kind the
        // mode declares.
        let kinds = &self.sections.slots;
        let mut slotted = BTreeSet::new();
        for (kind, ids) in &unit_type.section_or_default::<TypeSlotsData>().0 {
            kinds
                .names()
                .named(kind.as_str())
                .ok_or_else(|| unknown(at, kind, NameKind::SlotKind))?;
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
                if actions[id].kind == ActionKind::Train
                    && unit_type.section::<ProductionData>().is_none()
                {
                    return Err(LoadProblem::NoQueue(id.clone()));
                }
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

    /// The pools a unit type at `at` lists: none twice, and the life pool among them exactly
    /// when it has `combat`: with it, it dies at zero life; without it, it would stay a target
    /// at zero life forever.
    fn unit_pools(
        &self,
        pools: &[DeclaredName],
        combat: bool,
        at: &Place,
    ) -> Result<(), LoadProblem> {
        for (place, name) in pools.iter().enumerate() {
            if pools[..place].contains(name) {
                return Err(LoadProblem::Repeated {
                    at: at.clone(),
                    name: name.to_string(),
                });
            }
        }
        let lives = (self.sections.combat.life.as_ref()).is_some_and(|life| pools.contains(life));
        if lives && !combat {
            return Err(LoadProblem::CombatMissing(at.clone()));
        }
        Ok(())
    }

    /// Each name of `action`'s cost is a pool or a player resource the mode declares.
    fn cost_names(&self, id: &DeclaredName, action: &ActionData) -> Result<(), LoadProblem> {
        if let Some(name) = action
            .cost_names()
            .find(|name| self.names.cost_target(name.as_str()).is_none())
        {
            return Err(LoadProblem::Unknown {
                of: NameKind::Cost,
                at: Place::Entry {
                    table: ActionsData::KEY,
                    name: id.clone(),
                },
                name: name.to_string(),
            });
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
            .all(|tag| self.names.has(TableKind::Tag, tag.name.as_str()))
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

impl EffectScope<'_> {
    /// A spawn of `unit_type`, at `at`: a unit type of the action's package that stands and has
    /// no box; `fail` makes the problem of the effect's list.
    fn spawn(
        self,
        unit_type: &DeclaredName,
        at: &Place,
        fail: impl Fn(EffectProblem) -> LoadProblem,
    ) -> Result<(), LoadProblem> {
        let Some(spawned) = self
            .units
            .get(unit_type)
            .filter(|unit_type| !unit_type.delivers())
        else {
            return Err(unknown(at, unit_type, NameKind::UnitType));
        };
        if (spawned.section::<CollisionData>()).is_some_and(|collision| collision.form.is_box()) {
            return Err(fail(EffectProblem::SpawnBox));
        }
        Ok(())
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

/// The splash, the scatter and the speed of `action`, whose id is `id`: a splash goes off where a
/// projectile of its delivery ends, a scatter aims its delivery's curve at a point, and a speed
/// delays the hit of a weapon of no delivery (04-capabilities/combat.md, An attack may splash;
/// 04-capabilities/combat.md, An attack may scatter; 04-capabilities/combat.md, A slow weapon
/// delays its damage).
fn weapon_fields_hold(
    id: &DeclaredName,
    action: &ActionData,
    units: &BTreeMap<DeclaredName, UnitTypeData>,
) -> Result<(), LoadProblem> {
    let projectile = match &action.delivery {
        Some(delivery @ DeliveryData::Projectile { .. }) => units
            .get(delivery.unit_type())
            .and_then(|unit_type| unit_type.section::<ProjectileData>()),
        Some(DeliveryData::Area { .. }) | None => None,
    };
    if action.splash.is_some() && projectile.is_none() {
        return Err(LoadProblem::Delivery(DeliveryProblem::Splash(id.clone())));
    }
    let curve = projectile.is_some_and(|projectile| projectile.curve.is_some());
    if action.scatter.is_some() && !curve {
        return Err(LoadProblem::Delivery(DeliveryProblem::Scatter(id.clone())));
    }
    if action.speed.is_some() && action.delivery.is_some() {
        return Err(LoadProblem::Delivery(DeliveryProblem::Speed(id.clone())));
    }
    Ok(())
}

/// The `delivery` of `action`, whose id is `id` and whose script defines `on_hit` when
/// `hooks_hit`: a unit type of its package's `units` with the section of its kind. A projectile
/// needs an aim, homes alone and at a unit, flies a curve alone and at a unit or a point, and hits
/// a unit when the action has an `on_hit` list or hook, as a curve always meets the unit it aims
/// at; an area lands on a point, a unit or the caster, not along a direction; a weapon's is a
/// homing projectile or a curve.
fn delivery_holds(
    id: &DeclaredName,
    action: &ActionData,
    delivery: &DeliveryData,
    units: &BTreeMap<DeclaredName, UnitTypeData>,
    hooks_hit: bool,
) -> Result<(), LoadProblem> {
    let fail = |problem: fn(DeclaredName) -> DeliveryProblem| {
        Err(LoadProblem::Delivery(problem(id.clone())))
    };
    let name = delivery.unit_type();
    let unit_type = units.get(name).ok_or_else(|| LoadProblem::Unknown {
        of: NameKind::UnitType,
        at: Place::Entry {
            table: ActionsData::KEY,
            name: id.clone(),
        },
        name: name.to_string(),
    })?;
    let wrong_section = || {
        Err(LoadProblem::Delivery(DeliveryProblem::WrongSection {
            action: id.clone(),
            unit_type: name.clone(),
        }))
    };
    let flies_at_unit = match delivery {
        DeliveryData::Projectile { count, .. } => {
            let Some(projectile) = unit_type.section::<ProjectileData>() else {
                return wrong_section();
            };
            let at_unit = matches!(action.targeting, Targeting::Unit(_));
            if action.targeting == Targeting::None {
                return fail(DeliveryProblem::NoAim);
            }
            if projectile.homing && (!at_unit || count.get() > 1) {
                return fail(DeliveryProblem::Homing);
            }
            let along = action.targeting == Targeting::Direction;
            if projectile.curve.is_some() && (along || count.get() > 1) {
                return fail(DeliveryProblem::Curve);
            }
            if projectile.hits == ProjectileHits::Nothing
                && projectile.curve.is_none()
                && (hooks_hit || !action.on_hit.is_empty())
            {
                return fail(DeliveryProblem::NoHit);
            }
            projectile.flight().at_unit()
        }
        DeliveryData::Area { .. } => {
            if unit_type.section::<AreaData>().is_none() {
                return wrong_section();
            }
            if action.targeting == Targeting::Direction {
                return fail(DeliveryProblem::AreaDirection);
            }
            false
        }
    };
    if action.kind == ActionKind::Attack && !flies_at_unit {
        return fail(DeliveryProblem::Weapon);
    }
    Ok(())
}

/// Whether `unit_type`, a delivery type, is one and nothing more: of one section of a delivery
/// type, beside its tags, params, state and `vision`, and no section of a unit that stands.
fn delivery_only(unit_type: &UnitTypeData) -> bool {
    let readers = Release::sections();
    let delivers: Vec<&str> = readers.delivery_keys().collect();
    let beside = [
        TypeTagsData::KEY,
        TypeParamsData::KEY,
        TypeStateData::KEY,
        VisionData::KEY,
    ];
    let mut keys = unit_type.sections().keys();
    let delivered = keys.clone().filter(|key| delivers.contains(key)).count();
    delivered == 1 && keys.all(|key| delivers.contains(&key) || beside.contains(&key))
}

/// Whether `names` holds `name`.
fn declares<'n>(mut names: impl Iterator<Item = &'n DeclaredName>, name: &str) -> bool {
    names.any(|declared| declared.as_str() == name)
}

/// The unit types the `requires` of `action`, `id`, names of its package, `units`, stand: a
/// delivery type is none a player owns.
fn requires(
    id: &DeclaredName,
    action: &ActionData,
    units: &BTreeMap<DeclaredName, UnitTypeData>,
) -> Result<(), LoadProblem> {
    let Some(requires) = &action.requires else {
        return Ok(());
    };
    let at = Place::Entry {
        table: ActionsData::KEY,
        name: id.clone(),
    };
    if let Some(name) = requires
        .units
        .iter()
        .find(|name| units.get(*name).is_some_and(UnitTypeData::delivers))
    {
        return Err(unknown(&at, name, NameKind::UnitType));
    }
    Ok(())
}

fn unknown(at: &Place, name: &DeclaredName, of: NameKind) -> LoadProblem {
    LoadProblem::Unknown {
        at: at.clone(),
        name: name.to_string(),
        of,
    }
}
