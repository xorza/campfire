use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::slice;

use campfire_actions::{
    ActionData, ActionKind, ActionProblem, ActionRange, ActionSlots, DamageFields, DeliveryData,
    DoesData, EffectData, EffectRule, EffectTo, HealFields, KindData, LaunchFields, ListHook,
    MeteredKind, ModifierFields, MoveData, PurgeFields, RangeField, RestoreFields, SpawnFields,
    Targeting, XpFields,
};
use campfire_capabilities::{BookError, BookPlace, Books, Release, TypePlace, UnitTypeData};
use campfire_geometry::Metric;
use campfire_items::{ItemData, ItemError};
use campfire_math::Num;
use campfire_mode::{ModeList, ModeNamesError, Offers};
use campfire_progression::TrackError;
use campfire_projectiles::ProjectileHits;
use campfire_script::{
    ApiOwner, ApiVersion, CtxMisuse, DataTable, EnumRecord, EnumString, Hook, MemberKind, NameKind,
    ParamProblem, ScriptApi, ScriptFacts, ScriptFunction, ScriptName, ScriptRole, Status, SyncTo,
    UnknownName,
};
use campfire_sim::{Capability, TickRate};
use campfire_stats::{ModifierData, ModifierProblem, StatId};
use campfire_units::TypesError;
use campfire_values::{
    DeclaredName, EngineTag, FilterData, MessageId, Number, PackageIndex, PackagePath, Param, Stat,
};

use crate::dependent::{Dependent, DependentKind};
use crate::error::LoadError;
use crate::error::box_problem::BoxProblem;
use crate::error::build_problem::BuildProblem;
use crate::error::choice_problem::ChoiceProblem;
use crate::error::delivery_problem::DeliveryProblem;
use crate::error::effect_problem::EffectProblem;
use crate::error::gather_problem::GatherProblem;
use crate::error::item_problem::ItemProblem;
use crate::error::limit::Limit;
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

/// Design 08's checks at package load, over a mode and every package it depends on: data matches
/// its schema (the reads checked that), every per-rank array has an entry for each rank, every
/// script is named by data and defines only hooks of its roles, and every capability, `ctx` name,
/// modifier, param, stat, filter and damage kind a script or data names exists for the mode. And
/// every homing projectile is faster than the mode's move speed cap.
#[derive(Debug)]
pub(crate) struct LoadCheck<'a> {
    packages: &'a ModePackages,
    /// The names the mode's packages declare, each kind in its table.
    names: NameTables<'a>,
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

/// A unit type of one of the mode's packages: its place, its data, and its package's actions.
#[derive(Debug)]
struct PackageType<'p> {
    at: Place,
    unit_type: &'p UnitTypeData,
    actions: &'p BTreeMap<DeclaredName, ActionData>,
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
        let names = NameTables::of(packages);
        let check = LoadCheck {
            packages,
            names: names.map_err(|error| book_error(packages, error))?,
            engine,
            cap: manifest.max_move_speed.get(),
            api,
            rate: TickRate::new(manifest.tick_hz.fastest()),
            state_fields: LoadCheck::state_fields(packages),
        };
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
        let input = packages.book_input(check.rate, &stat_order);
        let books = Books::build(&input).map_err(|error| book_error(packages, error))?;
        books
            .check_walkable()
            .map_err(|problem| fail(LoadProblem::Map(problem)))?;
        // A name is the client's alone, so it is checked last: a train or a build whose other
        // problems the checks above find shows those.
        shown_named(&packages.content.actions).map_err(fail)?;
        for dependent in &packages.dependencies {
            shown_named(&dependent.content.actions)
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
        if data.players.bot_takeover && !data.players.late_join {
            return Err(LoadProblem::BotTakeoverWithoutLateJoin);
        }
        let owned = data
            .state
            .iter()
            .find(|(_, field)| matches!(field.sync, SyncTo::Owner | SyncTo::Team));
        if let Some((name, _)) = owned {
            return Err(LoadProblem::ModeStateSync(name.clone()));
        }
        self.sections_owned()?;
        self.names()?;
        self.damage_kinds()?;
        self.pools_and_resources()?;
        self.slot_kinds()?;
        self.choices()?;
        self.items()?;
        self.uses()?;
        for (name, unit_type) in units {
            let at = Place::UnitType(name.clone());
            self.unit_type(unit_type, &at, &content.actions, &content.modifiers)?;
        }
        let held = units.iter().filter_map(|(name, unit_type)| {
            Some((Place::UnitType(name.clone()), unit_type.passive.as_ref()?))
        });
        passives_held_once(held.chain(action_passives(&content.actions)))?;
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
        let mut names = PackageNames::new(&packages.mode, &content.modifiers, &content.actions);
        let mode_params: BTreeSet<&str> = data.params.keys().map(DeclaredName::as_str).collect();
        if let ModeScript::Own(path) = &packages.script {
            names.serve(path, ScriptRole::Mode, mode_params.iter().copied());
        }
        for unit_type in units.values() {
            if let Some(orders) = &unit_type.orders {
                names.serve(&orders.ai, ScriptRole::Ai, mode_params.iter().copied());
            }
        }
        let ranks = self.slotted_ranks(units.values())?;
        let ranks = |id: &str| ranks.get(id).copied().unwrap_or(1);
        self.actions(&content.actions, units, &mut names)?;
        let view = packages.mode();
        let ways = packages.modifier_ways(view);
        self.modifiers(&mut names, &ways, &content.actions, ranks)?;
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
        for (id, item) in &packages.content.items {
            message(mode, &item.name, &Place::Item(id.clone()))?;
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
        if !content.items.is_empty() {
            return Err(LoadProblem::Item(ItemProblem::OutsideMode));
        }
        let (actions, modifiers) = (&content.actions, &content.modifiers);
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
                if avatar.unit.orders.is_some() {
                    return Err(LoadProblem::AvatarOrders);
                }
                named(actions)?;
                self.unit_type(&avatar.unit, &at, actions, modifiers)?;
                let held = avatar.unit.passive.as_ref().map(|passive| (at, passive));
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
        let mode_params: BTreeSet<&str> = data.params.keys().map(DeclaredName::as_str).collect();
        if let ModeScript::Rules {
            package: name,
            path,
        } = &self.packages.script
            && *name == package.header.name
        {
            names.serve(path, ScriptRole::Mode, mode_params.iter().copied());
        }
        for unit_type in content.units.values() {
            if let Some(orders) = &unit_type.orders {
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
            let at = Place::Action(id.clone());
            if let Some(name) = &action.name {
                message(names.package, name, &at)?;
            }
            self.kind(id, action, units)?;
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
            let at = Place::Modifier(id.clone());
            self.require(Capability::Stats, &at)?;
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
            let modifiers = content
                .modifiers
                .values()
                .flat_map(|modifier| modifier.state.keys());
            let avatar = match view.kind {
                ViewKind::Avatar(avatar) => Some(&avatar.unit),
                ViewKind::Mode | ViewKind::Loadout | ViewKind::Rules => None,
            };
            let units = content.units.values().chain(avatar);
            let units = units.flat_map(|unit| unit.state.keys());
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
        let own = match kind {
            MemberKind::Field => {
                self.api.builtin(&format!("get${name}"))
                    || facts.map_keys.iter().any(|key| key == name)
            }
            _ => {
                self.api.builtin(name)
                    || facts.functions.iter().any(|function| function.name == name)
            }
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
            NameKind::Item => packages.content.items.contains_key(name),
            NameKind::Cost => self.names.cost_target(name).is_some(),
            NameKind::MarkerTag => {
                let spawns = declares(data.navigation.spawn_tags.iter(), name);
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
    const fn damage_kinds(&self) -> Result<(), LoadProblem> {
        let data = &self.packages.data;
        let kinds = &data.combat.damage_kinds;
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

    /// A `gather` of `production`, `id`: aimed at a unit.
    fn gather(&self, id: &DeclaredName, action: &ActionData) -> Result<(), LoadProblem> {
        let at = Place::Action(id.clone());
        self.require(Capability::Production, &at)?;
        if !matches!(action.targeting, Targeting::Unit(_)) {
            return Err(LoadProblem::Gather(GatherProblem::Aims(id.to_owned())));
        }
        Ok(())
    }

    /// A `build` of `production`, `id`: aimed at a point. That it builds a unit type of its
    /// package that stands, the book build holds.
    fn build(&self, id: &DeclaredName, action: &ActionData) -> Result<(), LoadProblem> {
        let at = Place::Action(id.clone());
        self.require(Capability::Production, &at)?;
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
        let at = Place::Action(id.clone());
        let run = matches!(
            action.kind,
            ActionKind::Cast
                | ActionKind::Attack
                | ActionKind::Use
                | ActionKind::Train
                | ActionKind::Build
                | ActionKind::Gather
        );
        let kind = run.then(|| KindData::of(action)).transpose();
        let kind = kind.map_err(|field| LoadProblem::KindField {
            action: id.to_owned(),
            field,
        })?;
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
                    if !unit_type.walks() {
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
        let at = Place::Action(id.clone());
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
                    if unit_type.area.is_none() {
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

    /// Each section of the mode's data and of its map that it holds belongs to a capability the
    /// mode declares, as each of a unit type's does: a match builds a section's grids, books and
    /// rules only for the capability that runs them. A section of the mode's data belongs to the
    /// capability the script API records as its reader; one of the map, to the capability that
    /// builds from it.
    fn sections_owned(&self) -> Result<(), LoadProblem> {
        let (data, map) = (&self.packages.data, &self.packages.map);
        let combat = &data.combat;
        let owner = |table, field| self.api.data_owner(table, field);
        let sections = [
            (
                !combat.damage_kinds.is_empty()
                    || combat.stats().next().is_some()
                    || combat.life.is_some(),
                owner(DataTable::Mode, "combat"),
                Place::Combat,
            ),
            (
                !data.stats.is_empty(),
                owner(DataTable::Mode, "stats"),
                Place::Stats,
            ),
            (
                !data.tracks.is_empty(),
                owner(DataTable::Mode, "tracks"),
                Place::Tracks,
            ),
            (
                !data.navigation.layers.is_empty(),
                owner(DataTable::ModeNavigation, "layers"),
                Place::Navigation,
            ),
            (
                data.supply.is_some(),
                owner(DataTable::Mode, "supply"),
                Place::Supply,
            ),
            (
                map.navigation.is_some(),
                Some(Capability::Navigation),
                Place::MapNavigation,
            ),
            (
                !map.paths.is_empty(),
                Some(Capability::Navigation),
                Place::Paths,
            ),
            (map.grid.is_some(), Some(Capability::Vision), Place::MapGrid),
        ];
        for (held, capability, at) in sections {
            if let Some(capability) = capability.filter(|_| held) {
                self.require(capability, &at)?;
            }
        }
        Ok(())
    }

    /// The mode's pools and player resources: no pool named as a resource, a pool only with
    /// `stats`; and with `combat`, a life pool.
    fn pools_and_resources(&self) -> Result<(), LoadProblem> {
        let data = &self.packages.data;
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
        for name in data.pools.keys() {
            self.require(Capability::Stats, &Place::Pool(name.clone()))?;
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
            .collision
            .as_ref()
            .is_some_and(|collision| collision.form.is_box());
        if boxed && self.packages.map.metric == Metric::Spatial {
            let at = at.clone();
            return Err(LoadProblem::BoxBody {
                at,
                problem: BoxProblem::Spatial,
            });
        }
        Ok(())
    }

    /// The capability of each section `unit_type` holds, at `at`, the mode declares; and a
    /// `supply` needs the mode's `[supply]`.
    fn sections(&self, unit_type: &UnitTypeData, at: &Place) -> Result<(), LoadProblem> {
        for field in unit_type.given() {
            if let Some(capability) = self.api.data_owner(DataTable::UnitType, field) {
                self.require(capability, at)?;
            }
        }
        if unit_type.supply.is_some() && self.packages.data.supply.is_none() {
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
    /// nor wide, and an area's `inside` modifiers of `modifiers`.
    fn unit_type(
        &self,
        unit_type: &UnitTypeData,
        at: &Place,
        actions: &BTreeMap<DeclaredName, ActionData>,
        modifiers: &BTreeMap<DeclaredName, ModifierData>,
    ) -> Result<(), LoadProblem> {
        self.sections(unit_type, at)?;
        self.own_tags(&unit_type.tags, at)?;
        self.body_box(unit_type, at)?;
        if unit_type.delivers() && !unit_type.delivery_only() {
            return Err(LoadProblem::Delivery(DeliveryProblem::NotDelivery(
                at.clone(),
            )));
        }
        if let Some(area) = &unit_type.area {
            // The modifiers' ways, which the modifiers' checks read, pass on through an area's
            // `inside` modifiers.
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
            let hits_aside = projectile.homing
                || projectile.stop_on_hit
                || projectile.once_per_cast
                || projectile.width > Num::ZERO;
            if projectile.hits == ProjectileHits::Nothing && hits_aside {
                return Err(LoadProblem::Delivery(DeliveryProblem::HitsNothing(
                    at.clone(),
                )));
            }
        }
        self.unit_pools(&unit_type.pools, unit_type.combat.is_some(), at)?;
        // The slotted ranks, which the actions' checks read, hold only the slots of a kind the
        // mode declares.
        let kinds = &self.packages.data.slots;
        let mut slotted = BTreeSet::new();
        for (kind, ids) in &unit_type.slots {
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
                if actions[id].kind == ActionKind::Train && unit_type.production.is_none() {
                    return Err(LoadProblem::NoQueue(id.clone()));
                }
            }
        }
        LoadCheck::slot_count(unit_type, at)?;
        self.inventory(unit_type, at)?;
        self.shop(unit_type, at)?;
        Ok(())
    }

    /// The inventory of a unit type at `at`, if it has one: it fills a slot kind the mode declares,
    /// of one rank, as an item's action has.
    fn inventory(&self, unit_type: &UnitTypeData, at: &Place) -> Result<(), LoadProblem> {
        let Some(inventory) = &unit_type.inventory else {
            return Ok(());
        };
        let kinds = &self.packages.data.slots;
        // A kind the mode does not declare is the book build's problem.
        let Some(kind) = kinds.names().named(inventory.kind.as_str()) else {
            return Ok(());
        };
        if kinds.ranks(kind) != 1 {
            return Err(LoadProblem::Item(ItemProblem::RankedInventory {
                at: at.clone(),
                kind: inventory.kind.clone(),
            }));
        }
        Ok(())
    }

    /// The slots of a unit type at `at`, its own, its inventory's and its use slot, are no more
    /// than a unit holds.
    fn slot_count(unit_type: &UnitTypeData, at: &Place) -> Result<(), LoadProblem> {
        let own: usize = unit_type.slots.values().map(Vec::len).sum();
        let inventory = unit_type
            .inventory
            .as_ref()
            .map_or(0, |inventory| NonZeroUsize::from(inventory.slots).get());
        let uses = usize::from(unit_type.uses.is_some());
        if own + inventory + uses > ActionSlots::LIMIT {
            return Err(LoadProblem::TooManySlots(at.clone()));
        }
        Ok(())
    }

    /// The mode's item types: an item costs in the mode's player resources, is built from items
    /// of the mode and never from itself, and holds no modifier that is another's passive; and no
    /// unit type or choice fills an inventory's slot kind. Item types need `items`. Its costs
    /// against its stack and its components the item book holds.
    fn items(&self) -> Result<(), LoadProblem> {
        let packages = self.packages;
        let content = &packages.content;
        let items = &content.items;
        for (id, item) in items {
            let at = Place::Item(id.clone());
            self.require(Capability::Items, &at)?;
            // The component loop and the components' cost, below, read each resource and each
            // component.
            let resources = self.names.resources();
            let resource = |name: &&DeclaredName| resources.named(name.as_str()).is_none();
            if let Some(name) = item.cost.keys().find(resource) {
                return Err(unknown(&at, name, NameKind::Resource));
            }
            if let Some(name) = item
                .components
                .iter()
                .find(|name| !items.contains_key(*name))
            {
                return Err(unknown(&at, name, NameKind::Item));
            }
        }
        let mode_passives = content.units.iter().filter_map(|(name, unit_type)| {
            Some((Place::UnitType(name.clone()), unit_type.passive.as_ref()?))
        });
        for (owner, modifier) in mode_passives.chain(action_passives(&content.actions)) {
            let held = items
                .iter()
                .find(|(_, item)| item.modifiers.contains(modifier));
            if let Some((item, _)) = held {
                return Err(LoadProblem::SharedPassive {
                    modifier: modifier.clone(),
                    owners: [owner, Place::Item(item.clone())],
                });
            }
        }
        self.inventory_kinds()?;
        if let Some(looped) = component_loop(items) {
            return Err(LoadProblem::Item(ItemProblem::ComponentLoop(
                looped.clone(),
            )));
        }
        Ok(())
    }

    /// The shop of a unit type at `at`, if it is one: each item it sells costs only in the one
    /// player resource it takes. A name the mode does not have is the book build's problem.
    fn shop(&self, unit_type: &UnitTypeData, at: &Place) -> Result<(), LoadProblem> {
        let Some(shop) = &unit_type.shop else {
            return Ok(());
        };
        if self
            .names
            .resources()
            .named(shop.resource.as_str())
            .is_none()
        {
            return Ok(());
        }
        let items = &self.packages.content.items;
        let other = |item: &&DeclaredName| {
            items
                .get(*item)
                .is_some_and(|item| item.cost.keys().any(|resource| *resource != shop.resource))
        };
        if let Some(item) = shop.items.iter().find(other) {
            return Err(LoadProblem::Item(ItemProblem::ShopResource {
                at: at.clone(),
                item: item.clone(),
            }));
        }
        Ok(())
    }

    /// No unit type of any package and no choice puts actions in a slot kind whose slots an
    /// inventory fills: those slots are the items'.
    fn inventory_kinds(&self) -> Result<(), LoadProblem> {
        let typed = self.unit_types();
        let kinds: BTreeSet<&DeclaredName> = typed
            .iter()
            .filter_map(|typed| Some(&typed.unit_type.inventory.as_ref()?.kind))
            .collect();
        let slotted = self.slotted_in(&typed, &kinds);
        slotted.map_or(Ok(()), |(at, kind)| {
            Err(LoadProblem::Item(ItemProblem::InventoryKindSlotted {
                at,
                kind,
            }))
        })
    }

    /// The uses: each `use` section names an action of its package of the `use` kind, and nothing
    /// else names one, no unit type's slots, no item and no loadout entry, as a use runs only from
    /// its object; and each `uses` names a slot kind of one rank, as a use's action has, that no
    /// inventory fills, and in which no unit type and no choice puts actions, as its slot is the
    /// uses'. A name the package or the mode does not have is the book build's problem.
    fn uses(&self) -> Result<(), LoadProblem> {
        let packages = self.packages;
        let kinds = &packages.data.slots;
        let typed = self.unit_types();
        let inventories: BTreeSet<&DeclaredName> = typed
            .iter()
            .filter_map(|typed| Some(&typed.unit_type.inventory.as_ref()?.kind))
            .collect();
        let is_use = |actions: &BTreeMap<DeclaredName, ActionData>, id: &DeclaredName| {
            actions.get(id).map(|action| action.kind == ActionKind::Use)
        };
        let mut held = BTreeSet::new();
        for PackageType {
            at,
            unit_type,
            actions,
        } in &typed
        {
            if let Some(usable) = &unit_type.usable
                && is_use(actions, &usable.action) == Some(false)
            {
                return Err(LoadProblem::Use(UseProblem::NotUse {
                    at: at.clone(),
                    action: usable.action.clone(),
                }));
            }
            if let Some(id) = unit_type
                .slots
                .values()
                .flatten()
                .find(|id| is_use(actions, id) == Some(true))
            {
                return Err(LoadProblem::Use(UseProblem::Named {
                    at: at.clone(),
                    action: id.clone(),
                }));
            }
            let Some(uses) = &unit_type.uses else {
                continue;
            };
            let Some(kind) = kinds.names().named(uses.kind.as_str()) else {
                continue;
            };
            let at = at.clone();
            let name = uses.kind.clone();
            if kinds.ranks(kind) != 1 {
                return Err(LoadProblem::Use(UseProblem::RankedKind { at, kind: name }));
            }
            if inventories.contains(&uses.kind) {
                return Err(LoadProblem::Use(UseProblem::InventoryKind {
                    at,
                    kind: name,
                }));
            }
            held.insert(&uses.kind);
        }
        if let Some((at, kind)) = self.slotted_in(&typed, &held) {
            return Err(LoadProblem::Use(UseProblem::KindSlotted { at, kind }));
        }
        let content = &packages.content;
        let items = content
            .items
            .iter()
            .filter_map(|(id, item)| Some((Place::Item(id.clone()), item.action.as_ref()?)));
        let loadouts = packages
            .dependencies
            .iter()
            .filter(|dependent| matches!(dependent.kind, DependentKind::Loadout))
            .flat_map(|dependent| dependent.content.actions.iter())
            .filter(|(_, action)| action.kind == ActionKind::Use)
            .map(|(id, _)| (Place::Loadouts, id));
        let named = items
            .filter(|(_, id)| is_use(&content.actions, id) == Some(true))
            .chain(loadouts)
            .next();
        named.map_or(Ok(()), |(at, action)| {
            Err(LoadProblem::Use(UseProblem::Named {
                at,
                action: action.clone(),
            }))
        })
    }

    /// Every unit type of the mode's packages, each with its place and its package's actions: the
    /// mode's, each avatar, and each type a dependency holds, an avatar's summon among them.
    fn unit_types(&self) -> Vec<PackageType<'a>> {
        let packages = self.packages;
        let mode = packages
            .content
            .units
            .iter()
            .map(|(name, unit_type)| PackageType {
                at: Place::UnitType(name.clone()),
                unit_type,
                actions: &packages.content.actions,
            });
        let dependents = packages.dependencies.iter().flat_map(|dependent| {
            let actions = &dependent.content.actions;
            let avatar = match &dependent.kind {
                DependentKind::Avatar(avatar) => Some(PackageType {
                    at: Place::Avatar(dependent.package.header.name.clone()),
                    unit_type: &avatar.unit,
                    actions,
                }),
                DependentKind::Loadout | DependentKind::Rules => None,
            };
            let held = dependent
                .content
                .units
                .iter()
                .map(move |(name, unit_type)| PackageType {
                    at: Place::UnitType(name.clone()),
                    unit_type,
                    actions,
                });
            avatar.into_iter().chain(held)
        });
        mode.chain(dependents).collect()
    }

    /// The first of `typed` and of the mode's choices that puts actions in a slot kind of `kinds`,
    /// with that kind.
    fn slotted_in(
        &self,
        typed: &[PackageType<'_>],
        kinds: &BTreeSet<&DeclaredName>,
    ) -> Option<(Place, DeclaredName)> {
        let types = typed.iter().filter_map(|typed| {
            let kind = typed
                .unit_type
                .slots
                .keys()
                .find(|kind| kinds.contains(kind))?;
            Some((typed.at.clone(), kind.clone()))
        });
        let choices = self
            .packages
            .data
            .choices
            .iter()
            .filter_map(|(name, choice)| {
                let kind = choice.slot.as_ref().filter(|kind| kinds.contains(kind))?;
                Some((Place::Choice(name.clone()), kind.clone()))
            });
        types.chain(choices).next()
    }

    /// No data gives a field design 08 plans: the mode's `state_version`, or a projectile type's
    /// `gravity` in any package. An action's planned effects fail the action's own checks.
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
            let falls = unit_type
                .projectile
                .as_ref()
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

    /// The mode's slot kinds: none that gives its ranks levels in a mode with no `level` track to
    /// read them on. Their count and names the decode holds.
    fn slot_kinds(&self) -> Result<(), LoadProblem> {
        let data = &self.packages.data;
        let kinds = data.slots.kinds();
        let level_track = data.tracks.values().any(|track| track.level);
        let leveled = kinds.iter().find(|kind| {
            kind.ranks
                .as_ref()
                .is_some_and(|ranks| ranks.levels().is_some())
        });
        match leveled {
            Some(kind) if !level_track => Err(LoadProblem::RankLevels(kind.name.clone())),
            _ => Ok(()),
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
                let at = Place::Choice(name.clone());
                data.slots
                    .names()
                    .named(kind.as_str())
                    .ok_or_else(|| unknown(&at, kind, NameKind::SlotKind))
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

    /// The pools a unit type at `at` lists: none twice, and the life pool among them exactly
    /// when it has `combat`: with it, it dies at zero life; without it, it would stay a target
    /// at zero life forever.
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

    /// Each name of `action`'s cost is a pool or a player resource the mode declares.
    fn cost_names(&self, id: &DeclaredName, action: &ActionData) -> Result<(), LoadProblem> {
        if let Some(name) = action
            .cost_names()
            .find(|name| self.names.cost_target(name.as_str()).is_none())
        {
            return Err(LoadProblem::Unknown {
                of: NameKind::Cost,
                at: Place::Action(id.clone()),
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
    let at = |package: PackageIndex, unit_type: TypePlace| match unit_type {
        TypePlace::Avatar => {
            let view = packages.packages().nth(usize::from(package));
            let package = &view.expect("the books name a package").package;
            Place::Avatar(package.header.name.clone())
        }
        TypePlace::Declared(name) => Place::UnitType(name),
    };
    let (package, problem) = match error {
        BookError::Start {
            package,
            unit_type,
            error,
        } => (
            package,
            LoadProblem::UnitStart {
                at: at(package, unit_type),
                error,
            },
        ),
        BookError::BoxWalks { package, unit_type } => (
            package,
            LoadProblem::BoxBody {
                at: at(package, unit_type),
                problem: BoxProblem::Walks,
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
            problem,
        } => (package, action_problem(action, problem)),
        BookError::Modifier {
            package,
            modifier,
            problem,
        } => (package, LoadProblem::Modifier { modifier, problem }),
        BookError::Mode(error) => (PackageIndex::MODE, LoadProblem::Mode(error)),
        BookError::Names(error) => (PackageIndex::MODE, names_problem(error)),
        BookError::Types(error) => (PackageIndex::MODE, types_problem(error)),
        BookError::Script { package, path } => (
            package,
            LoadProblem::Script {
                path,
                problem: ScriptProblem::Missing,
            },
        ),
        BookError::Tracks(TrackError::TooMany) => {
            (PackageIndex::MODE, LoadProblem::TooMany(Limit::Tracks))
        }
        BookError::Tracks(TrackError::LevelTracks) => {
            (PackageIndex::MODE, LoadProblem::LevelTracks)
        }
        BookError::Item {
            package,
            item,
            error,
        } => (
            package,
            LoadProblem::Item(match error {
                ItemError::StackCost(_) => ItemProblem::StackCost(item),
                ItemError::CheaperThanComponents(_) => ItemProblem::CheaperThanComponents(item),
            }),
        ),
        BookError::AreaTime { package, unit_type } => (
            package,
            LoadProblem::Delivery(DeliveryProblem::AreaTime(unit_type)),
        ),
        BookError::Unknown {
            package,
            at: place,
            name,
        } => {
            let at = match place {
                BookPlace::UnitType(unit_type) => at(package, unit_type),
                BookPlace::Action(action) => Place::Action(action),
                BookPlace::Modifier(modifier) => Place::Modifier(modifier),
                BookPlace::Pool(pool) => Place::Pool(pool),
                BookPlace::Item(item) => Place::Item(item),
                BookPlace::Combat => Place::Combat,
            };
            let UnknownName { of, name } = name;
            (package, LoadProblem::Unknown { at, name, of })
        }
    };
    let view = packages.packages().nth(usize::from(package));
    LoadError::of(
        &view.expect("the books name a package").package.header.name,
        problem,
    )
}

/// The problem of `error`, the unit types or tags that the book build refused.
fn types_problem(error: TypesError) -> LoadProblem {
    match error {
        TypesError::TooManyTags => LoadProblem::TooMany(Limit::Tags),
        TypesError::TooManyTypes => LoadProblem::TooMany(Limit::UnitTypes),
        TypesError::Repeated(name) => LoadProblem::Repeated {
            at: Place::UnitTypes,
            name,
        },
    }
}

/// The problem of `error`, a list of the mode's names that the book build refused.
fn names_problem(error: ModeNamesError) -> LoadProblem {
    match error {
        ModeNamesError::TooMany(list) => LoadProblem::TooMany(match list {
            ModeList::Pools => Limit::Pools,
            ModeList::Resources => Limit::Resources,
            ModeList::DamageKinds => Limit::DamageKinds,
            // Each layer is a tag of the match.
            ModeList::Layers => Limit::Tags,
        }),
        ModeNamesError::Repeated { list, name } => {
            let at = match list {
                ModeList::Pools => Place::Pool(name.clone()),
                ModeList::Resources => Place::Resources,
                ModeList::DamageKinds => Place::Combat,
                ModeList::Layers => Place::Navigation,
            };
            let name = name.to_string();
            LoadProblem::Repeated { at, name }
        }
    }
}

/// The problem of `problem`, a rule the data of `action` breaks, as the book build found it.
fn action_problem(action: DeclaredName, problem: ActionProblem) -> LoadProblem {
    match problem {
        ActionProblem::Time(error) => LoadProblem::Action { action, error },
        ActionProblem::RankCount(ranks) => LoadProblem::RankCount { action, ranks },
        ActionProblem::SlottedRanks => LoadProblem::ActionRanks(action),
        ActionProblem::Field(field) => LoadProblem::ActionField { action, field },
        ActionProblem::KindField(field) => LoadProblem::KindField { action, field },
        ActionProblem::Bounce => LoadProblem::Gather(GatherProblem::Bounce(action)),
        ActionProblem::Global(MeteredKind::Gather) => {
            LoadProblem::Gather(GatherProblem::Global(action))
        }
        ActionProblem::Global(MeteredKind::Build) => {
            LoadProblem::Build(BuildProblem::Global(action))
        }
        ActionProblem::Global(MeteredKind::Use) => LoadProblem::Use(UseProblem::Global(action)),
        ActionProblem::Section(unit_type) => {
            LoadProblem::Delivery(DeliveryProblem::WrongSection { action, unit_type })
        }
        ActionProblem::NoBox => LoadProblem::Build(BuildProblem::NoBox(action)),
        ActionProblem::StartLife => LoadProblem::Build(BuildProblem::StartLife(action)),
        ActionProblem::Effect { list, rule } => LoadProblem::Effect {
            action,
            list,
            problem: match rule {
                EffectRule::Planned(planned) => EffectProblem::Planned(planned),
                EffectRule::Overflow => EffectProblem::Overflow,
                EffectRule::Negative => EffectProblem::Negative,
                EffectRule::Duration => EffectProblem::Duration,
            },
        },
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
        if spawned
            .collision
            .as_ref()
            .is_some_and(|collision| collision.form.is_box())
        {
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
    ) -> PackageNames<'a> {
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
            Some(param) => param.numbers().any(negative),
            None => by
                .iter()
                .filter_map(|action| action.params.get(&reference.param))
                .any(|param| param.numbers().any(negative)),
        },
    })
}

/// The passive of each of `actions`, by its place.
fn action_passives(
    actions: &BTreeMap<DeclaredName, ActionData>,
) -> impl Iterator<Item = (Place, &DeclaredName)> {
    actions.iter().filter_map(|(id, action)| {
        Some((Place::Action(id.clone()), action.passive_modifier.as_ref()?))
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

/// The `delivery` of `action`, whose id is `id` and whose script defines `on_hit` when
/// `hooks_hit`: a unit type of its package's `units` with the section of its kind. A projectile
/// needs an aim, homes alone and at a unit, and hits a unit when the action has an `on_hit` list
/// or hook; an area lands on a point, a unit or the caster, not along a direction; a weapon's is
/// a homing projectile.
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
            if projectile.hits == ProjectileHits::Nothing
                && (hooks_hit || !action.on_hit.is_empty())
            {
                return fail(DeliveryProblem::NoHit);
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
    let at = Place::Action(id.clone());
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

/// An item of `items` built, through its components, from itself, if one is: the first a walk of
/// each item's components in id order meets again on its own path.
fn component_loop(items: &BTreeMap<DeclaredName, ItemData>) -> Option<&DeclaredName> {
    /// Where the walk is with an item: not met, on the path, or done with every item it is built
    /// from.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Open,
        OnPath,
        Done,
    }
    fn walk<'i>(
        id: &'i DeclaredName,
        items: &'i BTreeMap<DeclaredName, ItemData>,
        marks: &mut BTreeMap<&'i DeclaredName, Mark>,
    ) -> Option<&'i DeclaredName> {
        match marks.get(id).copied().unwrap_or(Mark::Open) {
            Mark::OnPath => return Some(id),
            Mark::Done => return None,
            Mark::Open => {}
        }
        marks.insert(id, Mark::OnPath);
        let components = items
            .get(id)
            .map_or(&[][..], |item| item.components.as_slice());
        for component in components {
            if let Some(looped) = walk(component, items, marks) {
                return Some(looped);
            }
        }
        marks.insert(id, Mark::Done);
        None
    }
    let mut marks = BTreeMap::new();
    items.keys().find_map(|id| walk(id, items, &mut marks))
}
