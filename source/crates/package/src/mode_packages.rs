use std::collections::{BTreeMap, BTreeSet};
use std::iter;
use std::path::Path;

use campfire_capabilities::{
    BookInput, BookKind, BookPackage, CollisionData, DeclaredName, EngineStat, EngineTag, MapData,
    ModeData, PackageContent, Param, ScriptApi, ScriptBook, Stat, StatGraph, StatsData,
    UnitTypeFile, Walker,
};
use campfire_content::{Fingerprint, MessageId, PackagePath};
use campfire_script::ScriptHost;
use campfire_sim::{Capability, TickRate};

use crate::error::{ContentError, Limit, LoadError, LoadProblem, PackageRef, StoreError};
use crate::files::avatar_data::AvatarData;
use crate::files::manifest::{Manifest, ModeManifest};
use crate::files::mode_file::ModeFile;
use crate::files::units_data::UnitsData;
use crate::load_check::LoadCheck;
use crate::package::Package;
use crate::package_dir::PackageDir;
use crate::package_files::PackageFiles;
use crate::package_index::PackageIndex;
use crate::package_store::PackageStore;

const MODE_DATA: &str = "data/mode.toml";
const UNITS_DATA: &str = "data/units.toml";
const MAP_DATA: &str = "map/map.toml";
const AVATAR_DATA: &str = "data/avatar.toml";
const LOADOUT_DATA: &str = "data/loadout.toml";

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

/// A package the mode depends on: it, its content, and its kind.
#[derive(Debug)]
pub struct Dependent {
    pub package: Package,
    pub content: PackageContent,
    pub kind: DependentKind,
}

/// The kind of a package the mode depends on: an avatar, with its one unit type, or a loadout.
#[derive(Debug)]
pub enum DependentKind {
    Avatar(Box<AvatarUnit>),
    Loadout,
}

/// An avatar's unit type, and the message of the name players see for it.
#[derive(Debug)]
pub struct AvatarUnit {
    pub name: MessageId,
    pub unit: UnitTypeFile,
}

/// One of a mode's packages, as every package is: its place, it, its content, and its kind.
#[derive(Debug, Clone, Copy)]
pub struct PackageView<'a> {
    pub index: PackageIndex,
    pub package: &'a Package,
    pub content: &'a PackageContent,
    pub kind: ViewKind<'a>,
}

/// The kind of one of a mode's packages.
#[derive(Debug, Clone, Copy)]
pub enum ViewKind<'a> {
    Mode,
    Avatar(&'a AvatarUnit),
    Loadout,
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

    /// Its avatars' unit types, each with its package's view.
    /// What its books are built from at `rate`, its scripts' hooks as `scripts` gives them, in
    /// the order a match compiles them.
    pub fn book_input<'a>(&'a self, rate: TickRate, scripts: &'a ScriptBook) -> BookInput<'a> {
        let packages = self.packages().map(|view| BookPackage {
            name: &view.package.name,
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
            max_move_speed: self.manifest.max_move_speed,
            progression: self.manifest.capabilities.contains(Capability::Progression),
            tag_names: self.tag_names().into_iter().collect(),
            packages: packages.collect(),
            scripts,
            rate,
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
            .map(|dependent| dependent.package.name.as_str())
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
        let move_speed = Stat::Engine(EngineStat::MoveSpeed);
        let navigation = &self.data.navigation;
        let walker = |stats: Option<&StatsData>, collision: Option<&CollisionData>| {
            stats
                .is_some_and(|stats| stats.declares(&move_speed))
                .then(|| Walker::of(navigation.body(collision).as_ref()))
        };
        let avatars = self.avatars().map(|avatar| &avatar.unit);
        let mut walkers: Vec<Walker> = self
            .content
            .units
            .values()
            .chain(avatars)
            .filter_map(|unit_type| walker(unit_type.stats.as_ref(), unit_type.collision.as_ref()))
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
        let api = ScriptApi::release();
        let name = manifest.header.name.clone();
        let fail = |problem| LoadError::of(&name, problem);
        if PackageIndex::dependency(dependencies.len()).is_none() {
            return Err(fail(LoadProblem::TooMany(Limit::Packages)));
        }
        let ModeFile { data, mut content } = files
            .read_data(&path(MODE_DATA))
            .map_err(content_error)
            .map_err(fail)?;
        let units: UnitsData = files
            .read_data(&path(UNITS_DATA))
            .map_err(content_error)
            .map_err(fail)?;
        content.units = units.units;
        let map = files
            .read_data(&path(MAP_DATA))
            .map_err(content_error)
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

impl Dependent {
    /// The package of `files`, which the mode names `name`: an avatar or loadout package of that
    /// name.
    fn read(
        name: &str,
        files: &PackageFiles,
        parser: &ScriptHost,
        api: &ScriptApi,
    ) -> Result<Dependent, LoadError> {
        let fail = |problem| LoadError::of(name, problem);
        let manifest: Manifest = files
            .read_data(&path(PackageDir::MANIFEST))
            .map_err(content_error)
            .map_err(fail)?;
        let header = manifest.header();
        if header.name != name {
            return Err(fail(LoadProblem::OtherName(header.name.clone())));
        }
        let (content, kind) = match &manifest {
            Manifest::Avatar(_) => {
                let AvatarData {
                    name,
                    unit,
                    content,
                } = files
                    .read_data(&path(AVATAR_DATA))
                    .map_err(content_error)
                    .map_err(fail)?;
                let avatar = AvatarUnit { name, unit };
                (content, DependentKind::Avatar(Box::new(avatar)))
            }
            Manifest::Loadout(_) => {
                let content = files.read_data(&path(LOADOUT_DATA));
                let content = content.map_err(content_error).map_err(fail)?;
                (content, DependentKind::Loadout)
            }
            Manifest::Mode(_) | Manifest::Locale(_) => return Err(fail(LoadProblem::WrongKind)),
        };
        let package = Package::read(files, header, parser, api)?;
        Ok(Dependent {
            package,
            content,
            kind,
        })
    }
}

/// The mode manifest of `files`, the package `package`.
fn read_mode_manifest(
    files: &PackageFiles,
    package: &PackageRef,
) -> Result<ModeManifest, LoadError> {
    let fail = |problem| LoadError::new(package.clone(), problem);
    let manifest = files.read_data(&path(PackageDir::MANIFEST));
    match manifest.map_err(content_error).map_err(fail)? {
        Manifest::Mode(manifest) => Ok(manifest),
        _ => Err(fail(LoadProblem::WrongKind)),
    }
}

/// One of the engine's paths in a package.
fn path(path: &str) -> PackagePath {
    PackagePath::parse(path).expect("the engine's paths are in the package")
}

fn content_error(error: ContentError) -> LoadProblem {
    LoadProblem::Content(error)
}
