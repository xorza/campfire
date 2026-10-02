use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::iter;
use std::path::Path;

use campfire_capabilities::{
    BookInput, BookKind, BookPackage, Books, CapabilitySet, DeclaredName, EngineTag, MapData,
    ModeData, PackageContent, Param, ScriptBook, StatGraph, UnitTypeFile, Walker,
};
use campfire_content::Fingerprint;
use campfire_script::{ScriptHost, ScriptId};
use campfire_sim::{Capability, TickRate};

use crate::avatar_unit::AvatarUnit;
use crate::dependent::Dependent;
use crate::dependent::DependentKind;
use crate::error::{Limit, LoadError, LoadProblem, PackageRef, StoreError};
use crate::files::manifest::Manifest;
use crate::files::mode_file::ModeFile;
use crate::files::mode_manifest::ModeManifest;
use crate::files::units_data::UnitsData;
use crate::load_check::LoadCheck;
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
    /// The mode's actions, modifiers and unit types.
    pub(crate) content: PackageContent,
    /// In the order of their names in the mode's manifest.
    pub(crate) dependencies: Vec<Dependent>,
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

    /// The mode's actions, modifiers and unit types.
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

    /// What its books are built from at `rate`, its scripts' hooks as `scripts` gives them, in
    /// the order a match compiles them.
    /// The books of a match of its packages at `rate`, a rate within the manifest's range, its
    /// scripts' hooks as `scripts` gives them. The load built them at the fastest rate the range
    /// allows, where every time counts the most ticks, so they build at every rate a session may
    /// choose.
    pub fn books(&self, rate: TickRate, scripts: &ScriptBook) -> Books {
        assert!(
            self.manifest.tick_hz.contains(rate.hz()),
            "a session's rate is within the manifest's range"
        );
        Books::build(&self.book_input(rate, scripts))
            .unwrap_or_else(|error| panic!("the load built the books at the fastest rate: {error}"))
    }

    /// Compiles every script of its packages by `compile`, in the order a match compiles them;
    /// the load parsed each one, so none fails.
    pub fn compile_scripts<E: fmt::Display>(
        &self,
        mut compile: impl FnMut(&str) -> Result<ScriptId, E>,
    ) {
        for view in self.packages() {
            for script in &view.package.scripts {
                compile(&script.source)
                    .unwrap_or_else(|error| panic!("the load parsed {}: {error}", script.path));
            }
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

    pub fn book_input<'a>(&'a self, rate: TickRate, scripts: &'a ScriptBook) -> BookInput<'a> {
        let packages = self.packages().map(|view| BookPackage {
            name: &view.package.header.name,
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
            tag_names: self.tag_names().into_iter().collect(),
            packages: packages.collect(),
            scripts,
            rate,
            stat_order: self
                .stat_graph()
                .order()
                .expect("the load checked the stat graph"),
        }
    }

    /// The hooks each script of its packages defines, in the order a match compiles them: the
    /// mode's scripts, then each dependency's, each package's in the order of their paths.
    pub fn script_book(&self) -> ScriptBook {
        let mut book = ScriptBook::default();
        for view in self.packages() {
            for script in &view.package.scripts {
                let functions = script.facts.functions.iter();
                book.push(functions.map(|function| (function.name.as_str(), function.params)));
            }
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

    /// Which stats each live stat change reads and which it changes, across the modifiers of the
    /// mode and of each package it depends on: a change that reads a param its modifier, or else
    /// an ability that applies it, declares as a scaling table reads each stat the table names.
    pub fn stat_graph(&self) -> StatGraph {
        let mut graph = StatGraph::new(self.data.stats.keys().cloned());
        for view in self.packages() {
            let content = view.content;
            let appliers = view.package.appliers(&content.actions, &content.units);
            for (id, modifier) in &content.modifiers {
                let by = appliers.get(id.as_str()).map_or(&[][..], Vec::as_slice);
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

    /// Each kind of unit that walks, of the mode's unit types and avatars that declare a move
    /// speed, by its layer and its body's radius, 0 for one with no body; in order, each once.
    /// The pathing grid has a clearance for each.
    pub fn walkers(&self) -> Vec<Walker> {
        let navigation = &self.data.navigation;
        let walker = |unit_type: &UnitTypeFile| {
            let body = navigation.body(unit_type.collision.as_ref());
            unit_type.walks().then(|| Walker::of(body.as_ref()))
        };
        let avatars = self.avatars().map(|avatar| &avatar.unit);
        let mut walkers: Vec<Walker> = self
            .content
            .units
            .values()
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
    pub fn tag_names(&self) -> BTreeSet<&str> {
        let modifiers = self
            .packages()
            .flat_map(|view| view.content.modifiers.values())
            .flat_map(|modifier| &modifier.tags);
        let avatars = self.avatars().map(|avatar| &avatar.unit);
        let types = self
            .packages()
            .flat_map(|view| view.content.units.values())
            .chain(avatars)
            .flat_map(|unit_type| &unit_type.core.tags);
        let declared = self
            .data
            .tags
            .iter()
            .flat_map(|(name, tag)| [name].into_iter().chain(&tag.immune));
        modifiers
            .chain(types)
            .chain(declared)
            .chain(&self.data.navigation.layers)
            .map(DeclaredName::as_str)
            .filter(|name| EngineTag::named(name).is_none())
            .collect()
    }

    /// The packages it depends on, in the order of their names in its manifest.
    pub fn dependencies(&self) -> &[Dependent] {
        &self.dependencies
    }

    /// Reads the mode's data and each dependency, and runs the load checks.
    fn assemble(
        files: &PackageFiles,
        manifest: ModeManifest,
        dependencies: &[(String, &PackageFiles)],
    ) -> Result<ModePackages, LoadError> {
        let parser = ScriptHost::new(manifest.script_limits.per_call);
        let api = CapabilitySet::script_api();
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
        let dependencies = dependencies
            .iter()
            .map(|(name, files)| Dependent::read(name, files, &parser, &api))
            .collect::<Result<_, _>>()?;
        let packages = ModePackages {
            mode,
            manifest,
            data,
            map,
            content,
            dependencies,
        };
        LoadCheck::run(&packages, &api)?;
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
