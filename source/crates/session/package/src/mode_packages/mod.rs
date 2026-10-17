use std::collections::{BTreeMap, BTreeSet};
use std::iter;
use std::sync::Arc;

use campfire_actions::{ActionData, SlotKinds, TypeSlotsData};
use campfire_capabilities::{BookInput, Books, PackageContent, PackageScript, Release};
use campfire_common::{DeclaredName, MapName, PackageFingerprint};
use campfire_declare::{SourceKind, SourcePackage, UnitTypeData};
use campfire_mode::{MapData, ModeData};
use campfire_navigation::ModeNavigationData;
use campfire_script::{ScriptHost, ScriptId, UnknownName};
use campfire_sim::TickRate;
use campfire_stats::{EdgeError, ModeStatsData, StatGraph, StatId};
use campfire_units::{HeightGrid, ModeTagsData, ScriptBook, TypeTagsData};
use campfire_values::{EngineTag, Names, PackageIndex, PackagePath, UnitType};

use crate::dependent::{Dependent, DependentKind};
use crate::error::limit::Limit;
use crate::error::load_problem::LoadProblem;
use crate::error::place::Place;
use crate::error::{ContentError, LoadError};
use crate::file_index::FileIndex;
use crate::files::map_file::MapFile;
use crate::files::mode_file::ModeFile;
use crate::files::mode_manifest::ModeManifest;
use crate::files::mode_script::ModeScript;
use crate::files::package_name::PackageName;
use crate::files::units_file::UnitsFile;
use crate::mode_packages::load_check::LoadCheck;
use crate::modifier_ways::ModifierWays;
use crate::package::Package;
use crate::package_files::PackageFiles;
use crate::package_view::{PackageView, ViewKind};

mod load_check;
mod name_tables;

const MODE_DATA: &str = "data/mode.toml";
const UNITS_DATA: &str = "data/units.toml";

/// A mode and the packages it depends on, read and checked: what a match of the mode loads.
#[derive(Debug)]
pub struct ModePackages {
    pub(crate) mode: Package,
    pub(crate) manifest: ModeManifest,
    /// The script it runs as its own.
    pub(crate) script: ModeScript,
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
    /// Every tag its packages name but the engine's, in the order a match declares them.
    tag_names: Vec<DeclaredName>,
    /// The places of the mode's stats in the order the stats refresh computes them, which the
    /// load check found from the stat graph.
    stat_order: Vec<StatId>,
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
    pub fn books(&self, rate: TickRate) -> Books {
        assert!(
            self.manifest.tick_hz.contains(rate.hz()),
            "a session's rate is within the manifest's range"
        );
        let engine_tags = Release::engine_tags();
        Release::books(&self.book_input(rate, &self.stat_order, &engine_tags))
            .expect("the load built the books at the fastest rate")
    }

    /// The names of its unit types, each in its scope, as a match's books number them.
    pub fn unit_types(&self) -> Arc<Names<UnitType>> {
        let engine_tags = Release::engine_tags();
        let types = Books::unit_types(&engine_tags, self.tag_names(), &self.book_packages());
        Arc::clone(types.expect("the load declared every type").types())
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

    /// The mode's script, by its place among the scripts a match compiles: past every script of
    /// the packages before the one that holds it.
    pub fn mode_script(&self) -> ScriptId {
        let mut before = 0;
        for view in self.packages() {
            if self.runs_script_of(view) {
                let at = view
                    .package
                    .script_index(self.script.path())
                    .expect("the load checked the mode's script");
                return ScriptId::nth(before + at);
            }
            before += view.package.scripts.len();
        }
        panic!("the load checked the package of the mode's script")
    }

    /// Whether `view` is the package that holds the mode's script.
    fn runs_script_of(&self, view: PackageView<'_>) -> bool {
        match (self.script.package(), view.kind) {
            (None, ViewKind::Mode) => true,
            (Some(name), ViewKind::Rules) => view.package.header.name == *name,
            _ => false,
        }
    }

    /// What its books are built from at `rate`, its stats computed in `stat_order`.
    pub(crate) fn book_input<'a>(
        &'a self,
        rate: TickRate,
        stat_order: &'a [StatId],
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
            tag_names: &self.tag_names,
            packages: self.book_packages(),
            scripts: &self.scripts,
            rate,
            stat_order,
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

    /// The hooks each script of `packages` defines, in the order a match compiles them: the
    /// mode's scripts, then each dependency's, each package's in the order of their paths.
    fn read_hooks<'p>(packages: impl IntoIterator<Item = &'p Package>) -> ScriptBook {
        let mut book = ScriptBook::default();
        for script in packages.into_iter().flat_map(|package| &package.scripts) {
            let functions = script.facts.functions.iter();
            book.push(functions.map(|function| (function.name.as_str(), function.params)));
        }
        book
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

    /// The ways each modifier of `view`, one of its packages, is applied.
    pub(crate) fn modifier_ways<'a>(&'a self, view: PackageView<'a>) -> ModifierWays<'a> {
        let mode_script = self.runs_script_of(view).then_some(self.script.path());
        ModifierWays::of(view, mode_script)
    }

    /// Which stats each live stat change reads and which it changes, across the modifiers of the
    /// mode and of each package it depends on: a change that reads a param its modifier, or else
    /// an action of a way that applies it, declares as a scaling table reads each stat the table
    /// names. An error names a stat the mode does not have, where the package gives it.
    pub(crate) fn stat_graph(&self) -> Result<StatGraph, LoadError> {
        let stats = self.data.section_or_default::<ModeStatsData>();
        let mut graph = StatGraph::new(stats.0.keys().cloned());
        for view in self.packages() {
            let content = view.content;
            let ways = self.modifier_ways(view);
            let fail = |at, name: UnknownName| {
                let UnknownName { of, name } = name;
                let problem = LoadProblem::Unknown { at, name, of };
                LoadError::of(&view.package.header.name, problem)
            };
            for (id, modifier) in content.modifiers() {
                let by: Vec<(&DeclaredName, &ActionData)> =
                    ways.actions_of(id.as_str(), content.actions()).collect();
                for (changed, change) in &modifier.stats {
                    let Some(name) = change.value.param() else {
                        continue;
                    };
                    let own = modifier.params.get(name).map(|param| (None, param));
                    let applied = by.iter().filter(|_| own.is_none());
                    let applied = applied
                        .filter_map(|&(action, data)| Some((Some(action), data.params.get(name)?)));
                    for (action, param) in own.into_iter().chain(applied) {
                        for read in param.stats() {
                            graph.add(read, changed).map_err(|error| match error {
                                EdgeError::Read(name) if let Some(action) = action => {
                                    fail(Place::Action(action.clone()), name)
                                }
                                EdgeError::Read(name) | EdgeError::Changed(name) => {
                                    fail(Place::Modifier(id.clone()), name)
                                }
                            })?;
                        }
                    }
                }
            }
        }
        Ok(graph)
    }

    /// The ranks of each action `types` place in their slots: those of the slot kind it sits in.
    /// An error names an action they place in kinds of other ranks. A kind the mode does not
    /// declare places nothing.
    pub(crate) fn slotted_ranks<'u>(
        &self,
        types: impl IntoIterator<Item = &'u UnitTypeData>,
    ) -> Result<BTreeMap<&'u str, u8>, &'u DeclaredName> {
        let types = types.into_iter();
        let slots = types.filter_map(|unit_type| unit_type.section::<TypeSlotsData>());
        let kinds = self.data.section_or_default::<SlotKinds>();
        kinds.slotted_ranks(slots.map(|slots| &slots.0))
    }

    /// Every tag its packages name but the engine's, each once, sorted: the tags of its unit
    /// types, its avatars' and its dependencies' delivery types, the names of its layers, those
    /// its and its dependencies' modifiers grant, and those of its `[tags]` and their
    /// immunities. A match declares them in this order after the engine's, so it numbers them
    /// the same however it loads.
    pub fn tag_names(&self) -> &[DeclaredName] {
        &self.tag_names
    }

    /// The tag names of a mode of `mode`'s data and content, and of `dependencies`; see
    /// `tag_names`.
    fn collect_tag_names(
        mode: (&ModeData, &PackageContent),
        dependencies: &[Dependent],
    ) -> Vec<DeclaredName> {
        let (data, content) = mode;
        let mode_tags = data.section_or_default::<ModeTagsData>();
        let navigation = data.section_or_default::<ModeNavigationData>();
        let engine = Release::engine_tags();
        let contents =
            iter::once(content).chain(dependencies.iter().map(|dependent| &dependent.content));
        let modifiers = contents
            .clone()
            .flat_map(|content| content.modifiers().values())
            .flat_map(|modifier| &modifier.tags);
        let avatars = dependencies
            .iter()
            .filter_map(|dependent| match &dependent.kind {
                DependentKind::Avatar(avatar) => Some(&avatar.unit),
                DependentKind::Loadout | DependentKind::Rules => None,
            });
        let types = contents
            .flat_map(|content| content.units.values())
            .chain(avatars)
            .filter_map(|unit_type| unit_type.section::<TypeTagsData>())
            .flat_map(|tags| &tags.0);
        let declared =
            (mode_tags.0.iter()).flat_map(|(name, tag)| [name].into_iter().chain(&tag.immune));
        let names: BTreeSet<&DeclaredName> = modifiers
            .chain(types)
            .chain(declared)
            .chain(&navigation.layers)
            .filter(|name| EngineTag::named(&engine, name.as_str()).is_none())
            .collect();
        names.into_iter().cloned().collect()
    }

    /// The packages it depends on, in the order of their names in its manifest.
    pub fn dependencies(&self) -> &[Dependent] {
        &self.dependencies
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
            return Err(fail(LoadProblem::TooMany(Limit::Packages)));
        }
        let ModeFile {
            script,
            data,
            mut content,
        } = files
            .read_data(&PackageFiles::engine_path(MODE_DATA))
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        let units: UnitsFile = files
            .read_data(&PackageFiles::engine_path(UNITS_DATA))
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        content.units = units.units;
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
        let map_data = PackagePath::parse(&format!("map/{map_name}/map.toml"))
            .expect("a map's name is a package path's name");
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
        let dependencies: Vec<Dependent> = dependencies
            .iter()
            .map(|(name, files)| Dependent::read(name, files, &parser, &api))
            .collect::<Result<_, _>>()?;
        if let Some(name) = script.package() {
            let rules = dependencies.iter().any(|dependent| {
                dependent.package.header.name == *name
                    && matches!(dependent.kind, DependentKind::Rules)
            });
            if !rules {
                return Err(fail(LoadProblem::ScriptPackage(name.clone())));
            }
        }
        let dependents = dependencies.iter().map(|dependent| &dependent.package);
        let scripts = Self::read_hooks(iter::once(&mode).chain(dependents));
        let tag_names = Self::collect_tag_names((&data, &content), &dependencies);
        let mut packages = Self {
            mode,
            manifest,
            script,
            data,
            map_name,
            map,
            heights,
            content,
            tag_names,
            dependencies,
            scripts,
            stat_order: Vec::new(),
        };
        packages.stat_order = LoadCheck::run(&packages, &api)?;
        Ok(packages)
    }
}

/// The names of the maps of the mode whose index is `index`: each directory `map/<name>/` that
/// holds a `map.toml`, in order; one whose name no map can have is refused.
fn maps_of(index: &FileIndex) -> Result<Vec<MapName>, LoadProblem> {
    index
        .rows()
        .filter_map(|(path, _)| {
            let name = path
                .as_str()
                .strip_prefix("map/")?
                .strip_suffix("/map.toml")?;
            (!name.contains('/')).then_some(name)
        })
        .map(|name| MapName::parse(name).ok_or_else(|| LoadProblem::MapDirName(name.to_owned())))
        .collect()
}
