use std::collections::BTreeMap;
use std::iter;
use std::num::NonZeroU32;
use std::sync::{Arc, OnceLock};

use campfire_capabilities::{BookInput, Books, PackageContent, PackageScript, Release};
use campfire_codec::TomlTable;
use campfire_common::{DeclaredName, MapName, PackageFingerprint};
use campfire_declarations::{ScriptAt, SourceKind, SourcePackage};
use campfire_mode::{MapData, ModeData, ModeParam};
use campfire_package_format::{ContentError, FileIndex, PackageFiles, PackageFinder, PackagePlace};
use campfire_script::{ModeScripts, ScriptApi, ScriptHost, ScriptId, ScriptUses};
use campfire_sim::{Capability, MatchRate};
use campfire_units::{HeightGrid, ScriptBook};
use campfire_values::{EngineTag, PackageIndex, PackagePath};

use crate::data_override::DataOverride;
use crate::dependent::{Dependent, DependentKind, OpenedDependent};
use crate::error::load_problem::LoadProblem;
use crate::error::{LoadError, StoreError};
use crate::files::bot_list::BotList;
use crate::files::map_file::MapFile;
use crate::files::mode_file::ModeFile;
use crate::files::mode_manifest::ModeManifest;
use crate::files::script_list::{ScriptOrder, ScriptSource};
use crate::files::units_file::UnitsFile;
use crate::mode_packages::load_check::LoadCheck;
use crate::mode_packages::mode_read::ModeRead;
use crate::package::Package;
use crate::package_name::PackageName;
use crate::package_view::{PackageView, ViewKind};

mod load_check;
pub(crate) mod mode_read;

/// A mode and the packages it depends on, read and checked: what a match of the mode loads.
#[derive(Debug)]
pub struct ModePackages {
    pub(crate) mode: Package,
    pub(crate) manifest: ModeManifest,
    /// The scripts it runs, in order, and which is its own.
    pub(crate) list: ScriptOrder,
    /// The bots it names, by name.
    pub(crate) bots: BotList,
    /// Each script of `list`, in its order, found once as the load checked it.
    listed: Vec<ListedScript>,
    /// The script of each of `bots`, by the bot's name.
    bot_scripts: BTreeMap<DeclaredName, ScriptId>,
    /// The books its load built, at the fastest rate, kept for what reads their names alone, as
    /// a book's rate changes no name.
    load_books: OnceLock<Arc<Books>>,
    pub(crate) data: ModeData,
    /// The name of the map it loaded, which its session plays.
    map_name: MapName,
    pub(crate) map: MapData,
    /// The map's height grid, `map/<name>/heights.bin`, when it has one.
    pub(crate) heights: Option<HeightGrid>,
    /// The mode's actions, modifiers, unit types and item types.
    pub(crate) content: PackageContent,
    /// In the order of their names in the mode's manifest.
    pub(crate) dependencies: Vec<Dependent>,
    /// The hooks each script defines, as the load read them, in the order a match compiles them.
    scripts: ScriptBook,
    /// What each script uses of the data, in the same order.
    uses: ScriptUses,
    overrides: Vec<DataOverride>,
}

/// A script of the mode's list: the package that holds it, and its place among the scripts a
/// match compiles.
#[derive(Debug, Clone, Copy)]
struct ListedScript {
    package: PackageIndex,
    id: ScriptId,
}

/// A bot a mode names, as a match runs it: its script, by its place among the scripts a match
/// compiles, the period of its think, and its params, which lie over the mode's.
#[derive(Debug, Clone, Copy)]
pub struct ModeBot<'a> {
    pub script: ScriptId,
    pub think_ms: NonZeroU32,
    pub params: &'a BTreeMap<DeclaredName, ModeParam>,
}

impl ModePackages {
    pub fn fingerprint(&self) -> PackageFingerprint {
        self.mode.fingerprint()
    }

    /// The fingerprints of its dependencies, in the order of their names in its manifest.
    pub fn dependency_fingerprints(
        &self,
    ) -> impl ExactSizeIterator<Item = PackageFingerprint> + '_ {
        self.dependencies
            .iter()
            .map(|dependent| dependent.package.fingerprint())
    }

    pub const fn manifest(&self) -> &ModeManifest {
        &self.manifest
    }

    pub const fn data(&self) -> &ModeData {
        &self.data
    }

    /// The mode's actions, modifiers, unit types and item types.
    pub const fn content(&self) -> &PackageContent {
        &self.content
    }

    /// The mode's own package.
    pub const fn mode(&self) -> PackageView<'_> {
        PackageView {
            index: PackageIndex::MODE,
            package: &self.mode,
            content: &self.content,
            kind: ViewKind::Mode,
        }
    }

    /// Every one of its packages, the mode's first, then each it depends on, in the order of
    /// their names in its manifest.
    pub fn packages(&self) -> impl Iterator<Item = PackageView<'_>> {
        let mode = self.mode();
        let dependents = self
            .dependencies
            .iter()
            .enumerate()
            .map(|(at, dependent)| PackageView {
                index: PackageIndex::dependency(at).expect("the load checked the package count"),
                package: &dependent.package,
                content: &dependent.content,
                kind: match &dependent.kind {
                    DependentKind::Avatar(avatar) => ViewKind::Avatar(avatar),
                    DependentKind::Loadout => ViewKind::Loadout,
                    DependentKind::Rules => ViewKind::Rules,
                },
            });
        iter::once(mode).chain(dependents)
    }

    /// The books of a match of its packages at `rate`, a rate within the manifest's range. The
    /// load built them at the fastest rate the range allows, where every time counts the most
    /// ticks, so they build at every rate a session may choose.
    pub fn books(&self, rate: MatchRate) -> Books {
        assert!(
            self.manifest.tick_hz.contains(rate.hz()),
            "a session's rate is within the manifest's range"
        );
        let engine_tags = Release::engine_tags();
        Release::books(&self.book_input(rate, &engine_tags))
            .expect("the load built the books at the fastest rate")
    }

    /// The books its load built, at the fastest rate, for what reads their names alone, as a
    /// bot's script does: a book's rate changes its times, never its names.
    pub fn load_books(&self) -> &Arc<Books> {
        self.load_books
            .get()
            .expect("the load keeps the books it built")
    }

    /// Every script of its packages, in the order a match compiles them, which numbers each at
    /// its place, as its book of hooks holds it; the load parsed each one.
    pub fn scripts(&self) -> Vec<PackageScript<'_>> {
        let scripts = self.packages().flat_map(|view| &view.package.scripts);
        scripts
            .map(|script| PackageScript {
                path: &script.path,
                source: &script.source,
            })
            .collect()
    }

    /// The bot `name`: its script, by its place among the scripts a match compiles, the period of
    /// its think, and its params; none for a bot the mode does not name.
    pub fn bot(&self, name: &DeclaredName) -> Option<ModeBot<'_>> {
        let bot = self.bots.get(name)?;
        Some(ModeBot {
            script: self.bot_scripts[name],
            think_ms: bot.think_ms,
            params: &bot.params,
        })
    }

    /// The scripts the mode's list names, in its order, each by its place among the scripts a
    /// match compiles.
    pub fn mode_scripts(&self) -> ModeScripts {
        let ids = self.listed.iter().map(|listed| listed.id);
        ModeScripts::new(ids, self.list.mode())
    }

    /// The script `source` of the mode's list or of a bot, found once its load checked it: the
    /// package that holds it, and its place among the scripts a match compiles, past every
    /// script of the packages before.
    fn find_script(&self, source: &ScriptSource) -> ListedScript {
        let mut before = 0;
        for view in self.packages() {
            if view.holds(source) {
                let at = view
                    .package
                    .script_index(source.path())
                    .expect("the load checked each script of the mode's list and its bots");
                return ListedScript {
                    package: view.index,
                    id: ScriptId::nth(before + at),
                };
            }
            before += view.package.scripts.len();
        }
        panic!("the load checked the package of each script of the mode's list and its bots")
    }

    /// The paths of the scripts of the mode's list that the package at `view` holds.
    pub(crate) fn listed_in<'s>(
        &'s self,
        view: PackageView<'s>,
    ) -> impl Iterator<Item = &'s PackagePath> {
        let held = self.list.iter().filter(move |source| view.holds(source));
        held.map(ScriptSource::path)
    }

    /// What its books are built from at `rate`.
    pub(crate) fn book_input<'a>(
        &'a self,
        rate: MatchRate,
        engine_tags: &'a [EngineTag],
    ) -> BookInput<'a> {
        BookInput {
            data: &self.data,
            map: &self.map,
            heights: self.heights.as_ref(),
            teams: &self.manifest.teams,
            max_move_speed: self.manifest.max_move_speed,
            set: self.manifest.capabilities,
            engine_tags,
            packages: self.book_packages(),
            scripts: &self.scripts,
            script_uses: &self.uses,
            mode_scripts: self
                .list
                .iter()
                .zip(&self.listed)
                .map(|(source, listed)| ScriptAt {
                    package: listed.package,
                    path: source.path(),
                })
                .collect(),
            tick_rate: rate,
        }
    }

    /// Its packages as its books are built from them, the mode's then each dependency's.
    pub(crate) fn book_packages(&self) -> Vec<SourcePackage<'_>> {
        let packages = self.packages().map(|view| SourcePackage {
            name: view.package.header.name.as_str(),
            units: &view.content.units,
            content: &view.content.sections,
            kind: match view.kind {
                ViewKind::Mode => SourceKind::Mode,
                ViewKind::Avatar(avatar) => SourceKind::Avatar(&avatar.unit),
                ViewKind::Loadout => SourceKind::Loadout,
                ViewKind::Rules => SourceKind::Rules,
            },
            scripts: view
                .package
                .scripts
                .iter()
                .map(|script| &script.path)
                .collect(),
        });
        packages.collect()
    }

    /// The hooks of `api` each script of `packages` defines, in the order a match compiles them:
    /// the mode's scripts, then each dependency's, each package's in the order of their paths.
    fn read_hooks<'p>(
        packages: impl IntoIterator<Item = &'p Package>,
        api: &ScriptApi,
    ) -> ScriptBook {
        let mut book = ScriptBook::new(api.hooks().iter().map(|status| status.hook));
        for script in packages.into_iter().flat_map(|package| &package.scripts) {
            let functions = script.facts.functions.iter();
            book.push(functions.map(|function| (function.name.as_str(), function.params)));
        }
        book
    }

    /// What each script of `packages` uses of the data, in the order a match compiles them.
    fn read_uses<'p>(packages: impl IntoIterator<Item = &'p Package>) -> ScriptUses {
        let mut uses = ScriptUses::default();
        for script in packages.into_iter().flat_map(|package| &package.scripts) {
            uses.push(&script.facts);
        }
        uses
    }

    /// The name of the map it loaded.
    pub const fn map_name(&self) -> &MapName {
        &self.map_name
    }

    pub const fn map(&self) -> &MapData {
        &self.map
    }

    /// The map's height grid, when it has one.
    pub const fn heights(&self) -> Option<&HeightGrid> {
        self.heights.as_ref()
    }

    /// The packages it depends on, in the order of their names in its manifest.
    pub fn dependencies(&self) -> &[Dependent] {
        &self.dependencies
    }

    /// The mode at `place` on its map `map`, or its only map when `map` names none, with each
    /// package its manifest names at its path from there, each read once.
    pub fn at(place: &impl PackagePlace, map: Option<&MapName>) -> Result<Self, LoadError> {
        ModeRead::at(place)?.on(map)
    }

    /// The mode of the fingerprint `mode` that `finder` holds on its map `map`, with each
    /// dependency its manifest names by the fingerprint of the same place in `dependencies`, as a
    /// verifier holds them.
    pub fn found(
        finder: &impl PackageFinder,
        mode: PackageFingerprint,
        map: &MapName,
        dependencies: &[PackageFingerprint],
    ) -> Result<Self, StoreError> {
        ModeRead::found(finder, mode, dependencies)?
            .on(Some(map))
            .map_err(StoreError::Load)
    }

    /// The mode of `files`, whose manifest is `manifest`, on its map `map`, or its only map when
    /// `map` names none, with each package its manifest names in `dependencies`, in the order of
    /// their names: checked as a match loads them, each file the load reads parsed once. A mode
    /// of several maps and none named fails, its error naming them all.
    pub fn new(
        files: &PackageFiles,
        manifest: ModeManifest,
        map: Option<&MapName>,
        dependencies: &[(PackageName, &PackageFiles)],
    ) -> Result<Self, LoadError> {
        let mut parser = ScriptHost::new(manifest.script_limits.per_call);
        let api = Release::bind_script_api(&mut parser);
        let name = manifest.header.name.clone();
        let fail = |problem| LoadError::of(&name, problem);
        if PackageIndex::dependency(dependencies.len()).is_none() {
            return Err(fail(LoadProblem::TooManyPackages));
        }
        let mode_data = ModeFile::path();
        let own: TomlTable = files
            .read_data(&mode_data)
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        let units_data = UnitsFile::path();
        let units: UnitsFile = files
            .read_data(&units_data)
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        let mut maps = maps_of(files.index()).map_err(fail)?;
        let map_name = match map {
            Some(map) if maps.contains(map) => map.clone(),
            Some(map) => return Err(fail(LoadProblem::NoMap(map.clone()))),
            None => match maps.pop() {
                Some(only) if maps.is_empty() => only,
                other => {
                    let named = maps.into_iter().chain(other).collect();
                    return Err(fail(LoadProblem::MapNeeded(named)));
                }
            },
        };
        let map_data = MapFile::path(&map_name);
        let MapFile(map) = files
            .reader()
            .read_data(&map_data)
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        let heights = match files.reader().read_binary(&HeightGrid::path(&map_name)) {
            Ok(grid) => Some(grid),
            Err(ContentError::Missing { .. }) => None,
            Err(error) => return Err(fail(LoadProblem::Content(error))),
        };
        let mode = Package::read(files, &manifest.header, &parser, &api)?;
        let mut opened: Vec<OpenedDependent<'_>> = dependencies
            .iter()
            .map(|(name, files)| Dependent::open(name, files))
            .collect::<Result<_, _>>()?;
        Self::check_needs(&manifest, &opened).map_err(fail)?;
        let mut overrides = Vec::new();
        let laid = Self::lay_data(own, &manifest.header.name, &mut opened, &mut overrides);
        let ModeFile {
            scripts: list,
            bots,
            defaults,
            data,
            mut content,
        } = laid
            .read()
            .map_err(|error| ContentError::Data {
                path: mode_data,
                error,
            })
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        content.units = units
            .read(&defaults)
            .map_err(|error| ContentError::Data {
                path: units_data,
                error,
            })
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        let dependencies: Vec<Dependent> = opened
            .into_iter()
            .map(|opened| opened.read(&parser, &api, &defaults))
            .collect::<Result<_, _>>()?;
        let bot_scripts = bots.iter().map(|(_, bot)| &bot.script);
        for name in list
            .iter()
            .chain(bot_scripts)
            .filter_map(ScriptSource::package)
        {
            let rules = dependencies.iter().any(|dependent| {
                dependent.package.header.name == *name
                    && matches!(dependent.kind, DependentKind::Rules)
            });
            if !rules {
                return Err(fail(LoadProblem::ScriptPackage(name.clone())));
            }
        }
        let dependents = dependencies.iter().map(|dependent| &dependent.package);
        let scripts = Self::read_hooks(iter::once(&mode).chain(dependents.clone()), &api);
        let uses = Self::read_uses(iter::once(&mode).chain(dependents));
        // The scripts of the list and of the bots are found once the load checked each.
        let mut packages = Self {
            mode,
            manifest,
            list,
            bots,
            listed: Vec::new(),
            bot_scripts: BTreeMap::new(),
            load_books: OnceLock::new(),
            data,
            map_name,
            map,
            heights,
            content,
            dependencies,
            scripts,
            uses,
            overrides,
        };
        let books = LoadCheck::run(&packages, &api)?;
        packages
            .load_books
            .set(Arc::new(books))
            .expect("the load builds its books once");
        packages.listed = packages
            .list
            .iter()
            .map(|source| packages.find_script(source))
            .collect();
        packages.bot_scripts = (&packages.bots)
            .into_iter()
            .map(|(name, bot)| (name.clone(), packages.find_script(&bot.script).id))
            .collect();
        Ok(packages)
    }

    /// Refuses a mode of `manifest` that lacks what a rules package of `dependencies` needs: a
    /// capability, or tick rates within its own (03-game-scripting.md, A rules package states
    /// what it needs).
    fn check_needs(
        manifest: &ModeManifest,
        dependencies: &[OpenedDependent<'_>],
    ) -> Result<(), LoadProblem> {
        for dependent in dependencies {
            let Some(needs) = dependent.needs() else {
                continue;
            };
            let package = dependent.name();
            let lacks = |capability: &&Capability| !manifest.capabilities.contains(**capability);
            if let Some(&capability) = needs.capabilities.iter().find(lacks) {
                return Err(LoadProblem::RulesCapability {
                    package: package.clone(),
                    capability,
                });
            }
            if needs
                .tick_hz
                .is_some_and(|bounds| !bounds.holds(manifest.tick_hz))
            {
                return Err(LoadProblem::RulesRate(package.clone()));
            }
        }
        Ok(())
    }

    /// The mode's data `own`, of the mode `mode`, laid over the data of each rules package of
    /// `dependencies` in their order, each over the ones before it, as the last loaded wins
    /// (03-game-scripting.md, Overrides by load order); each record more than one package gives
    /// goes to `overrides`, with the package whose value stays. The laid data reads as the mode's
    /// `data/mode.toml`, so a field of a rules package's data that does not read fails as that
    /// file.
    fn lay_data(
        own: TomlTable,
        mode: &PackageName,
        dependencies: &mut [OpenedDependent<'_>],
        overrides: &mut Vec<DataOverride>,
    ) -> TomlTable {
        let rules = dependencies.iter_mut().filter_map(|dependent| {
            let data = dependent.take_rules_data()?;
            Some((dependent.name(), data))
        });
        let layers = rules.chain(iter::once((mode, own)));
        let mut laid: Option<TomlTable> = None;
        for (by, mut table) in layers {
            if let Some(below) = laid.take() {
                let records = table.lay_over(below);
                overrides.extend(records.into_iter().map(|record| DataOverride {
                    record,
                    by: by.clone(),
                }));
            }
            laid = Some(table);
        }
        laid.expect("the mode's own data lies last")
    }

    /// Each record of mode data more than one of its packages gives, with the package whose value
    /// the mode takes, in the order the load laid them.
    pub fn overrides(&self) -> &[DataOverride] {
        &self.overrides
    }
}

/// The names of the maps of the mode whose index is `index`: each directory `map/<name>/` that
/// holds a `map.toml`, in order; one whose name no map can have is refused.
fn maps_of(index: &FileIndex) -> Result<Vec<MapName>, LoadProblem> {
    index
        .paths()
        .filter_map(MapFile::map_of)
        .map(|name| MapName::parse(name).ok_or_else(|| LoadProblem::MapDirName(name.to_owned())))
        .collect()
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_capabilities::Release;
    use campfire_sim::MatchRate;

    use crate::mode_packages::ModePackages;

    impl ModePackages {
        /// Every tag its packages name but the engine's, in the order a match declares them,
        /// after the engine's.
        pub fn tag_names(&self) -> Vec<String> {
            let engine_tags = Release::engine_tags();
            let rate = MatchRate::new(self.manifest.tick_hz.fastest());
            let types = Release::unit_types(&self.book_input(rate, &engine_tags));
            let types = types.expect("the load declared every tag");
            let names = types.tags().names().texts().skip(engine_tags.len());
            names.map(str::to_owned).collect()
        }
    }
}
