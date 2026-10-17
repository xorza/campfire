use std::collections::{BTreeMap, BTreeSet};

use campfire_actions::{ActionData, ActionKind};
use campfire_capabilities::{LoadNames, PackageContent, Release};
use campfire_common::DeclaredName;
use campfire_declare::{
    BookError, BookPlace, ScriptParams, SectionReaders, SectionTable, TypePlace,
};
use campfire_script::{
    ApiOwner, CtxMisuse, DataTable, EnumRecord, EnumString, Hook, MemberKind, NameKind, ScriptApi,
    ScriptFacts, ScriptFunction, ScriptName, ScriptRole, Status, UnknownName,
};
use campfire_sim::{Capability, TickRate};
use campfire_values::{FilterData, MessageId, PackageIndex, PackagePath};

use crate::dependent::{Dependent, DependentKind};
use crate::error::LoadError;
use crate::error::load_problem::LoadProblem;
use crate::error::place::Place;
use crate::error::script_problem::ScriptProblem;
use crate::mode_packages::ModePackages;
use crate::package::Package;
use crate::package_view::{PackageView, ViewKind};

/// The checks at package load (08-script-api.md, Checks at package load), over a mode and every
/// package it depends on: each section is of a capability the mode declares, the modules' rules
/// of their data hold, every script is named by data and defines only hooks of its roles, every
/// name a script uses is one the mode has, and every message data names is one its package's own
/// language gives.
#[derive(Debug)]
pub(crate) struct LoadCheck<'a> {
    packages: &'a ModePackages,
    /// The names the mode's packages declare, each kind as its owner builds it.
    names: &'a LoadNames,
    /// The script API of the release, which every name a script uses must be of.
    api: &'a ScriptApi,
}

/// The facts one package's checks share.
#[derive(Debug)]
struct PackageNames<'a> {
    package: &'a Package,
    content: &'a PackageContent,
    /// What data says of each script it names.
    scripts: BTreeMap<&'a PackagePath, ScriptUse<'a>>,
}

/// What data says of a script: the roles it serves, and the params its `ctx.p` may read.
#[derive(Debug, Default)]
struct ScriptUse<'a> {
    roles: BTreeSet<ScriptRole>,
    params: BTreeSet<&'a str>,
}

impl<'a> LoadCheck<'a> {
    /// Checks `packages` against the script API `api`.
    pub(crate) fn run(packages: &'a ModePackages, api: &'a ScriptApi) -> Result<(), LoadError> {
        let manifest = &packages.manifest;
        let fail = |problem| LoadError::of(&manifest.header.name, problem);
        Self::sections_owned(packages, &Release::sections())?;
        let engine = Release::engine_tags();
        let rate = TickRate::new(manifest.tick_hz.fastest());
        let input = packages.book_input(rate, &engine);
        // The modules' rules of their data come first, so a script that names what a broken rule
        // leaves out shows the rule.
        let names = Release::check_rules(&input).map_err(|error| book_error(packages, error))?;
        let check = LoadCheck {
            packages,
            names: &names,
            api,
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
        Ok(())
    }

    /// The mode package: the names players see of it, and what its data names.
    fn mode(&self) -> Result<(), LoadProblem> {
        self.names()?;
        self.package(self.packages.mode())
    }

    /// What the data of the package at `view` names: each message, of its own language's file,
    /// and each script, which serves its roles and reads the params they give; and its scripts.
    fn package(&self, view: PackageView<'a>) -> Result<(), LoadProblem> {
        let packages = self.packages;
        let of_package = |package: PackageIndex| package == view.index;
        for named in self.names.messages() {
            if of_package(named.package) {
                let at = book_place(packages, view.index, named.at.clone());
                message(view.package, &named.message, &at)?;
            }
        }
        let mut names = PackageNames::new(view.package, view.content);
        let mode_params: Vec<&str> = packages
            .data
            .param_names()
            .map(DeclaredName::as_str)
            .collect();
        if packages.runs_script_of(view) {
            let path = packages.script.path();
            names.serve(path, ScriptRole::Mode, mode_params.iter().copied());
        }
        for named in self.names.scripts() {
            if !of_package(named.package) {
                continue;
            }
            match &named.params {
                ScriptParams::Named(params) => {
                    let params = params.iter().map(DeclaredName::as_str);
                    names.serve(&named.path, named.role, params);
                }
                ScriptParams::Mode => {
                    names.serve(&named.path, named.role, mode_params.iter().copied());
                }
            }
        }
        self.scripts(&names)
    }

    /// The names players see of the mode, each team of its manifest and each choice: each is a
    /// message its own language's file gives, and every team has one.
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

    /// An avatar, loadout or rules package the mode depends on, at `view`: an avatar's name, and
    /// a name for each action of an avatar or a loadout, which a player casts; and its scripts,
    /// a rules package's the mode's when the mode names one of them.
    fn dependent(
        &self,
        view: PackageView<'a>,
        dependent: &'a Dependent,
    ) -> Result<(), LoadProblem> {
        let package = &dependent.package;
        let actions = dependent.content.actions();
        match &dependent.kind {
            DependentKind::Avatar(avatar) => {
                let at = Place::Avatar(package.header.name.clone());
                message(package, &avatar.name, &at)?;
                named(actions)?;
            }
            DependentKind::Loadout => named(actions)?,
            DependentKind::Rules => {}
        }
        self.package(view)
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
            let declared = |field: &&String| {
                self.packages.data.state.contains_key(field.as_str())
                    || self.names.state_fields().has(field)
            };
            if let Some(field) = facts.state_fields.iter().find(|field| !declared(field)) {
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
                NameKind::Modifier => package.content.modifiers().contains_key(name),
                NameKind::Ability => package.content.actions().contains_key(name),
                NameKind::Message => package.package.text.gives(name),
                NameKind::Param => package
                    .scripts
                    .get(path)
                    .is_some_and(|script| script.params.contains(name)),
                NameKind::Filter => return self.filter_text(name, &at),
                NameKind::Choice => declares(data.choices.keys(), name),
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

    /// No data gives a field the script API plans: the mode's `state_version`. A projectile's
    /// `gravity` and an action's planned effects fail the rules of their modules.
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
    const fn new(package: &'a Package, content: &'a PackageContent) -> Self {
        PackageNames {
            package,
            content,
            scripts: BTreeMap::new(),
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
    let shown =
        |action: &ActionData| matches!(action.kind(), ActionKind::Train | ActionKind::Build);
    match actions
        .iter()
        .find(|(_, action)| shown(action) && action.name.is_none())
    {
        Some((id, _)) => Err(LoadProblem::UnnamedAction(id.clone())),
        None => Ok(()),
    }
}

/// Whether `names` holds `name`.
fn declares<'n>(mut names: impl Iterator<Item = &'n DeclaredName>, name: &str) -> bool {
    names.any(|declared| declared.as_str() == name)
}
