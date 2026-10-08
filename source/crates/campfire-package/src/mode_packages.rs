use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::iter;
use std::path::Path;

use campfire_capabilities::{
    ActionData, BookInput, BookKind, BookPackage, Books, CapabilitySet, DeclaredName, EngineTag,
    MapData, ModeData, PackageContent, Param, ScriptBook, StatGraph, StatId, UnitTypeFile, Walker,
};
use campfire_common::Fingerprint;
use campfire_script::{ScriptHost, ScriptId};
use campfire_sim::{Capability, TickRate};

use crate::avatar_unit::AvatarUnit;
use crate::dependent::Dependent;
use crate::dependent::DependentKind;
use crate::error::limit::Limit;
use crate::error::load_problem::LoadProblem;
use crate::error::{LoadError, PackageRef, StoreError};
use crate::files::manifest::Manifest;
use crate::files::mode_file::ModeFile;
use crate::files::mode_manifest::ModeManifest;
use crate::files::package_name::PackageName;
use crate::files::units_data::UnitsData;
use crate::load_check::LoadCheck;
use crate::modifier_ways::ModifierWays;
use crate::package::Package;
use crate::package_dir::PackageDir;
use crate::package_files::PackageFiles;
use crate::package_index::PackageIndex;
use crate::package_store::PackageStore;
use crate::package_view::PackageView;
use crate::package_view::ViewKind;

const MODE_DATA: &str = "data/mode.toml";
const UNITS_DATA: &str = "data/units.toml";
const MAP_DATA: &str = "map/map.toml";

/// A mode and the packages it depends on, read and checked: what a match of the mode loads.
#[derive(Debug)]
pub struct ModePackages {
    pub(crate) mode: Package,
    pub(crate) manifest: ModeManifest,
    pub(crate) data: ModeData,
    pub(crate) map: MapData,
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
    /// The mode on disk in `dir`, and its dependencies at the paths its manifest gives, as a
    /// workspace holds them.
    pub fn from_dir(dir: &Path) -> Result<ModePackages, LoadError> {
        ModePackages::from_package_dir(&PackageDir::new(dir))
    }

    /// The mode in `mode`, and its dependencies at the paths its manifest gives from it, each
    /// read once.
    pub fn from_package_dir(mode: &PackageDir) -> Result<ModePackages, LoadError> {
        let read = |dir: &PackageDir, package: PackageRef| {
            dir.read()
                .map_err(|error| LoadError::new(package, LoadProblem::Content(error)))
        };
        let at = PackageRef::Dir(mode.root().to_owned());
        let files = read(mode, at.clone())?;
        let manifest = read_mode_manifest(&files, &at)?;
        let dependencies = manifest
            .dependencies
            .iter()
            .map(|(name, dependency)| {
                let dir = mode.join(Path::new(&dependency.path));
                Ok((name.clone(), read(&dir, PackageRef::Name(name.clone()))?))
            })
            .collect::<Result<Vec<_>, LoadError>>()?;
        let dependencies: Vec<_> = dependencies
            .iter()
            .map(|(name, files)| (name.clone(), files))
            .collect();
        ModePackages::assemble(&files, manifest, &dependencies)
    }

    /// The mode of the fingerprint `mode`, and each dependency its manifest names by the
    /// fingerprint of the same place in `dependencies`, from `store`, as a verifier holds them.
    pub fn from_store(
        store: &PackageStore,
        mode: Fingerprint,
        dependencies: &[Fingerprint],
    ) -> Result<ModePackages, StoreError> {
        let files = store.get(mode).ok_or(StoreError::UnknownMode)?;
        let manifest =
            read_mode_manifest(files, &PackageRef::Fingerprint(mode)).map_err(StoreError::Load)?;
        if manifest.dependencies.len() != dependencies.len() {
            return Err(StoreError::DependencyCount);
        }
        let dependencies = manifest
            .dependencies
            .keys()
            .zip(dependencies)
            .map(|(name, &fingerprint)| {
                let files = store
                    .get(fingerprint)
                    .ok_or_else(|| StoreError::MissingDependency(name.clone()))?;
                Ok((name.clone(), files))
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        ModePackages::assemble(files, manifest, &dependencies).map_err(StoreError::Load)
    }

    pub const fn fingerprint(&self) -> Fingerprint {
        self.mode.fingerprint
    }

    /// The fingerprints of its dependencies, in the order of their names in its manifest.
    pub fn dependency_fingerprints(&self) -> impl ExactSizeIterator<Item = Fingerprint> + '_ {
        self.dependencies
            .iter()
            .map(|dependent| dependent.package.fingerprint)
    }

    pub const fn manifest(&self) -> &ModeManifest {
        &self.manifest
    }

    pub const fn mode(&self) -> &Package {
        &self.mode
    }

    pub const fn data(&self) -> &ModeData {
        &self.data
    }

    /// The mode's actions, modifiers, unit types and item types.
    pub const fn content(&self) -> &PackageContent {
        &self.content
    }

    /// Every one of its packages, the mode's first, then each it depends on, in the order of
    /// their names in its manifest.
    pub fn packages(&self) -> impl Iterator<Item = PackageView<'_>> {
        let mode = PackageView {
            index: PackageIndex::MODE,
            package: &self.mode,
            content: &self.content,
            kind: ViewKind::Mode,
        };
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
        Books::build(&self.book_input(rate, &self.stat_order))
            .expect("the load built the books at the fastest rate")
    }

    /// Compiles every script of its packages by `compile`, in the order a match compiles them,
    /// which must number each at its place, as its book of hooks holds it; the load parsed each
    /// one, so none fails.
    pub fn compile_scripts<E: fmt::Debug>(
        &self,
        mut compile: impl FnMut(&str) -> Result<ScriptId, E>,
    ) {
        let scripts = self.packages().flat_map(|view| &view.package.scripts);
        for (at, script) in scripts.enumerate() {
            let id = compile(&script.source)
                .unwrap_or_else(|error| panic!("the load parsed {}: {error:?}", script.path));
            assert_eq!(
                id,
                ScriptId::nth(at),
                "the host numbers {} at its place",
                script.path
            );
        }
    }

    /// The mode's script, by its place among the scripts a match compiles.
    pub fn mode_script(&self) -> ScriptId {
        let at = self
            .mode
            .script_index(&self.data.script)
            .expect("the load checked the mode's script");
        ScriptId::nth(at)
    }

    /// What its books are built from at `rate`, its stats computed in `stat_order`.
    pub(crate) fn book_input<'a>(
        &'a self,
        rate: TickRate,
        stat_order: &'a [StatId],
    ) -> BookInput<'a> {
        let packages = self.packages().map(|view| BookPackage {
            name: view.package.header.name.as_str(),
            content: view.content,
            kind: match view.kind {
                ViewKind::Mode => BookKind::Mode,
                ViewKind::Avatar(avatar) => BookKind::Avatar(&avatar.unit),
                ViewKind::Loadout => BookKind::Loadout,
            },
            scripts: view
                .package
                .scripts
                .iter()
                .map(|script| &script.path)
                .collect(),
        });
        BookInput {
            data: &self.data,
            map: &self.map,
            teams: &self.manifest.teams,
            max_move_speed: self.manifest.max_move_speed,
            progression: self.manifest.capabilities.contains(Capability::Progression),
            tag_names: &self.tag_names,
            packages: packages.collect(),
            scripts: &self.scripts,
            rate,
            stat_order,
        }
    }

    /// The hooks each script of its packages defines, as the load read them, in the order a
    /// match compiles them.
    pub const fn script_book(&self) -> &ScriptBook {
        &self.scripts
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

    /// The names of its avatars' unit types in the mode's scope: their packages' names.
    pub(crate) fn avatar_names(&self) -> impl Iterator<Item = &str> {
        self.dependencies
            .iter()
            .filter(|dependent| matches!(dependent.kind, DependentKind::Avatar(_)))
            .map(|dependent| dependent.package.header.name.as_str())
    }

    fn avatars(&self) -> impl Iterator<Item = &AvatarUnit> {
        self.dependencies
            .iter()
            .filter_map(|dependent| match &dependent.kind {
                DependentKind::Avatar(avatar) => Some(&**avatar),
                DependentKind::Loadout => None,
            })
    }

    pub const fn map(&self) -> &MapData {
        &self.map
    }

    /// The ways each modifier of `view`, one of its packages, is applied.
    pub(crate) fn modifier_ways<'a>(&'a self, view: PackageView<'a>) -> ModifierWays<'a> {
        let mode_script = matches!(view.kind, ViewKind::Mode).then_some(&self.data.script);
        ModifierWays::of(view, mode_script)
    }

    /// Which stats each live stat change reads and which it changes, across the modifiers of the
    /// mode and of each package it depends on: a change that reads a param its modifier, or else
    /// an action of a way that applies it, declares as a scaling table reads each stat the table
    /// names.
    pub fn stat_graph(&self) -> StatGraph {
        let mut graph = StatGraph::new(self.data.stats.keys().cloned());
        for view in self.packages() {
            let content = view.content;
            let ways = self.modifier_ways(view);
            for (id, modifier) in &content.modifiers {
                let by: Vec<&ActionData> = ways.actions_of(id.as_str(), &content.actions).collect();
                for (changed, change) in &modifier.stats {
                    let Some(name) = change.value.param() else {
                        continue;
                    };
                    let own = modifier.params.get(name);
                    let applied = by.iter().filter(|_| own.is_none());
                    let applied = applied.filter_map(|ability| ability.params.get(name));
                    for read in own.into_iter().chain(applied).flat_map(Param::stats) {
                        graph.add(read, changed);
                    }
                }
            }
        }
        graph
    }

    /// The ranks of each action `types` place in their slots: those of the slot kind it sits in.
    /// An error names an action they place in kinds of other ranks. A kind the mode does not
    /// declare places nothing.
    pub fn slotted_ranks<'u>(
        &self,
        types: impl IntoIterator<Item = &'u UnitTypeFile>,
    ) -> Result<BTreeMap<&'u str, u8>, &'u DeclaredName> {
        let slots = types.into_iter().map(|unit_type| &unit_type.slots);
        self.data.slots.slotted_ranks(slots)
    }

    /// Each kind of unit that walks, of its packages' unit types and its avatars that declare a
    /// move speed, by its layer and its body's radius, 0 for one with no body; in order, each
    /// once.
    /// The pathing grid has a clearance for each.
    pub fn walkers(&self) -> Vec<Walker> {
        let navigation = &self.data.navigation;
        // A box never walks, which the load checked of each type that declares a move speed.
        let walker = |unit_type: &UnitTypeFile| {
            let form = navigation.form(unit_type.collision.as_ref());
            unit_type.walks().then(|| Walker::of_form(form)).flatten()
        };
        let avatars = self.avatars().map(|avatar| &avatar.unit);
        let mut walkers: Vec<Walker> = self
            .packages()
            .flat_map(|view| view.content.units.values())
            .chain(avatars)
            .filter_map(walker)
            .collect();
        walkers.sort_unstable();
        walkers.dedup();
        walkers
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
        let contents =
            iter::once(content).chain(dependencies.iter().map(|dependent| &dependent.content));
        let modifiers = contents
            .clone()
            .flat_map(|content| content.modifiers.values())
            .flat_map(|modifier| &modifier.tags);
        let avatars = dependencies
            .iter()
            .filter_map(|dependent| match &dependent.kind {
                DependentKind::Avatar(avatar) => Some(&avatar.unit),
                DependentKind::Loadout => None,
            });
        let types = contents
            .flat_map(|content| content.units.values())
            .chain(avatars)
            .flat_map(|unit_type| &unit_type.core.tags);
        let declared = data
            .tags
            .iter()
            .flat_map(|(name, tag)| [name].into_iter().chain(&tag.immune));
        let names: BTreeSet<&DeclaredName> = modifiers
            .chain(types)
            .chain(declared)
            .chain(&data.navigation.layers)
            .filter(|name| EngineTag::named(name.as_str()).is_none())
            .collect();
        names.into_iter().cloned().collect()
    }

    /// The packages it depends on, in the order of their names in its manifest.
    pub fn dependencies(&self) -> &[Dependent] {
        &self.dependencies
    }

    /// Reads the mode's data and each dependency, and runs the load checks.
    fn assemble(
        files: &PackageFiles,
        manifest: ModeManifest,
        dependencies: &[(PackageName, &PackageFiles)],
    ) -> Result<ModePackages, LoadError> {
        let mut parser = ScriptHost::new(manifest.script_limits.per_call);
        let api = CapabilitySet::bind_script_api(&mut parser);
        let name = manifest.header.name.clone();
        let fail = |problem| LoadError::of(&name, problem);
        if PackageIndex::dependency(dependencies.len()).is_none() {
            return Err(fail(LoadProblem::TooMany(Limit::Packages)));
        }
        let ModeFile { data, mut content } = files
            .read_data(&PackageDir::engine_path(MODE_DATA))
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        let units: UnitsData = files
            .read_data(&PackageDir::engine_path(UNITS_DATA))
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        content.units = units.units;
        let map = files
            .read_data(&PackageDir::engine_path(MAP_DATA))
            .map_err(LoadProblem::Content)
            .map_err(fail)?;
        let mode = Package::read(files, &manifest.header, &parser, &api)?;
        let dependencies: Vec<Dependent> = dependencies
            .iter()
            .map(|(name, files)| Dependent::read(name, files, &parser, &api))
            .collect::<Result<_, _>>()?;
        let dependents = dependencies.iter().map(|dependent| &dependent.package);
        let scripts = ModePackages::read_hooks(iter::once(&mode).chain(dependents));
        let tag_names = ModePackages::collect_tag_names((&data, &content), &dependencies);
        let mut packages = ModePackages {
            mode,
            manifest,
            data,
            map,
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

/// The mode manifest of `files`, the package `package`.
fn read_mode_manifest(
    files: &PackageFiles,
    package: &PackageRef,
) -> Result<ModeManifest, LoadError> {
    let fail = |problem| LoadError::new(package.clone(), problem);
    let manifest = files.read_data(&PackageDir::engine_path(PackageDir::MANIFEST));
    match manifest.map_err(LoadProblem::Content).map_err(fail)? {
        Manifest::Mode(manifest) => Ok(manifest),
        _ => Err(fail(LoadProblem::WrongKind)),
    }
}
