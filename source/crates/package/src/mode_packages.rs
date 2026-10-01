use std::collections::{BTreeMap, BTreeSet};
use std::iter;
use std::path::Path;

use campfire_capabilities::{
    CollisionData, DeclaredName, EngineStat, MapData, ModeData, Param, Stat, StatGraph, StatsData,
    UnitTypeData, Walker,
};
use campfire_content::{Fingerprint, PackagePath};
use campfire_script::ScriptHost;
use campfire_sim::Capability;

use crate::error::{ContentError, LoadError, LoadProblem, StoreError};
use crate::files::avatar_data::AvatarData;
use crate::files::loadout_data::LoadoutData;
use crate::files::manifest::{Manifest, ModeManifest};
use crate::files::units_data::{UnitTypeFile, UnitsData};
use crate::load_check::LoadCheck;
use crate::package::Package;
use crate::package_dir::PackageDir;
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
    pub(crate) units: UnitsData,
    pub(crate) map: MapData,
    /// In the order of their names in the mode's manifest.
    pub(crate) dependencies: Vec<Dependent>,
}

/// A package the mode depends on, and its data.
#[derive(Debug)]
pub struct Dependent {
    pub package: Package,
    pub content: Content,
}

/// The data of a package the mode depends on, by the package's kind.
#[derive(Debug)]
pub enum Content {
    Avatar(Box<AvatarData>),
    Loadout(LoadoutData),
}

impl ModePackages {
    /// The mode on disk in `dir`, and its dependencies at the paths its manifest gives, as a
    /// workspace holds them.
    pub fn from_dir(dir: &Path) -> Result<ModePackages, LoadError> {
        ModePackages::from_package_dir(&PackageDir::new(dir))
    }

    /// The mode in `mode`, and its dependencies at the paths its manifest gives from it.
    pub fn from_package_dir(mode: &PackageDir) -> Result<ModePackages, LoadError> {
        let manifest = read_mode_manifest(mode)?;
        let dependencies = manifest
            .dependencies
            .iter()
            .map(|(name, dependency)| (name.clone(), mode.join(Path::new(&dependency.path))))
            .collect::<Vec<_>>();
        ModePackages::assemble(mode, manifest, &dependencies)
    }

    /// The mode of the fingerprint `mode`, and each dependency its manifest names by the
    /// fingerprint of the same place in `dependencies`, from `store`, as a verifier holds them.
    pub fn from_store(
        store: &PackageStore,
        mode: Fingerprint,
        dependencies: &[Fingerprint],
    ) -> Result<ModePackages, StoreError> {
        let dir = store.get(mode).ok_or(StoreError::UnknownMode)?.clone();
        let manifest = read_mode_manifest(&dir).map_err(StoreError::Load)?;
        if manifest.dependencies.len() != dependencies.len() {
            return Err(StoreError::DependencyCount);
        }
        let dependencies = manifest
            .dependencies
            .keys()
            .zip(dependencies)
            .map(|(name, &fingerprint)| {
                let dir = store
                    .get(fingerprint)
                    .ok_or_else(|| StoreError::MissingDependency(name.clone()))?;
                Ok((name.clone(), dir.clone()))
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        ModePackages::assemble(&dir, manifest, &dependencies).map_err(StoreError::Load)
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

    pub const fn units(&self) -> &UnitsData {
        &self.units
    }

    pub const fn map(&self) -> &MapData {
        &self.map
    }

    /// Which stats each live stat change reads and which it changes, across the modifiers of the
    /// mode and of each package it depends on: a change that reads a param its modifier, or else
    /// an ability that applies it, declares as a scaling table reads each stat the table names.
    pub fn stat_graph(&self) -> StatGraph {
        let mut graph = StatGraph::new(self.data.stats.keys().cloned());
        let no_abilities = BTreeMap::new();
        let packages = iter::once((&self.mode, &no_abilities, &self.data.modifiers)).chain(
            self.dependencies
                .iter()
                .map(|dependent| match &dependent.content {
                    Content::Avatar(avatar) => {
                        (&dependent.package, &avatar.actions, &avatar.modifiers)
                    }
                    Content::Loadout(loadout) => {
                        (&dependent.package, &loadout.actions, &loadout.modifiers)
                    }
                }),
        );
        for (package, abilities, modifiers) in packages {
            let appliers = package.appliers(abilities);
            for (id, modifier) in modifiers {
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
    ) -> Result<BTreeMap<&'u str, u8>, &'u str> {
        let kinds = &self.data.slots;
        let mut ranks = BTreeMap::new();
        for unit_type in types {
            for (kind, ids) in &unit_type.slots {
                let Some(kind) = kinds.named(kind.as_str()) else {
                    continue;
                };
                for id in ids {
                    let held = *ranks.entry(id.as_str()).or_insert(kinds.ranks(kind));
                    if held != kinds.ranks(kind) {
                        return Err(id);
                    }
                }
            }
        }
        Ok(ranks)
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
        let unit_types =
            self.units.units.values().filter_map(|unit_type| {
                walker(unit_type.stats.as_ref(), unit_type.collision.as_ref())
            });
        let avatars = self
            .dependencies
            .iter()
            .filter_map(|dependent| match &dependent.content {
                Content::Avatar(avatar) => {
                    walker(avatar.unit.stats.as_ref(), avatar.unit.collision.as_ref())
                }
                Content::Loadout(_) => None,
            });
        let mut walkers: Vec<Walker> = unit_types.chain(avatars).collect();
        walkers.sort_unstable();
        walkers.dedup();
        walkers
    }

    /// Every tag its packages name, each once, sorted: `avatar`, `projectile` with projectiles,
    /// the tags of its unit types, its avatars' and its dependencies' delivery types, the names of
    /// its layers, those its and its dependencies' modifiers grant, and those of its `[tags]` and
    /// their immunities. A match declares them in this order, so it numbers them the same however
    /// it loads.
    pub fn tag_names(&self) -> BTreeSet<&str> {
        let dependents = self
            .dependencies
            .iter()
            .map(|dependent| match &dependent.content {
                Content::Avatar(avatar) => &avatar.modifiers,
                Content::Loadout(loadout) => &loadout.modifiers,
            });
        let modifiers = [&self.data.modifiers]
            .into_iter()
            .chain(dependents)
            .flat_map(|modifiers| modifiers.values())
            .flat_map(|modifier| &modifier.tags);
        let avatars = self
            .dependencies
            .iter()
            .filter_map(|dependent| match &dependent.content {
                Content::Avatar(avatar) => Some(&avatar.unit),
                Content::Loadout(_) => None,
            });
        let deliveries = self
            .dependencies
            .iter()
            .flat_map(|dependent| dependent.units().values());
        let types = self
            .units
            .units
            .values()
            .chain(avatars)
            .chain(deliveries)
            .flat_map(|unit_type| &unit_type.core.tags);
        let projectiles = self
            .manifest
            .capabilities
            .contains(Capability::Projectiles)
            .then_some(UnitTypeData::PROJECTILE_TAG);
        let declared = self
            .data
            .tags
            .iter()
            .flat_map(|(name, tag)| [name].into_iter().chain(&tag.immune));
        let layers = self.data.navigation.layers.iter().map(DeclaredName::as_str);
        modifiers
            .chain(types)
            .chain(declared)
            .map(String::as_str)
            .chain(layers)
            .chain([UnitTypeData::AVATAR_TAG])
            .chain(projectiles)
            .collect()
    }

    /// The packages it depends on, in the order of their names in its manifest.
    pub fn dependencies(&self) -> &[Dependent] {
        &self.dependencies
    }

    /// Reads the mode's data and each dependency, and runs the load checks.
    fn assemble(
        dir: &PackageDir,
        manifest: ModeManifest,
        dependencies: &[(String, PackageDir)],
    ) -> Result<ModePackages, LoadError> {
        let parser = ScriptHost::new(manifest.script_limits.per_call);
        let name = manifest.header.name.clone();
        let fail = |problem| LoadError {
            package: name.clone(),
            problem: Box::new(problem),
        };
        let data = dir
            .read_data(&path(MODE_DATA))
            .map_err(content)
            .map_err(fail)?;
        let units = dir
            .read_data(&path(UNITS_DATA))
            .map_err(content)
            .map_err(fail)?;
        let map = dir
            .read_data(&path(MAP_DATA))
            .map_err(content)
            .map_err(fail)?;
        let mode = Package::read(dir, name.clone(), manifest.header.engine, &parser)?;
        let dependencies = dependencies
            .iter()
            .map(|(name, dir)| Dependent::read(name, dir, &parser))
            .collect::<Result<_, _>>()?;
        let packages = ModePackages {
            mode,
            manifest,
            data,
            units,
            map,
            dependencies,
        };
        LoadCheck::run(&packages)?;
        Ok(packages)
    }
}

impl Dependent {
    /// The unit types it holds, by id: those its actions deliver.
    pub const fn units(&self) -> &BTreeMap<String, UnitTypeFile> {
        match &self.content {
            Content::Avatar(avatar) => &avatar.units,
            Content::Loadout(loadout) => &loadout.units,
        }
    }

    /// The name of its unit type `id` in a match: `<package>/<id>`, apart from every other
    /// package's.
    pub fn unit_type_name(&self, id: &str) -> String {
        format!("{}/{id}", self.package.name)
    }

    /// The package in `dir`, which the mode names `name`: an avatar or loadout package of that name.
    fn read(name: &str, dir: &PackageDir, parser: &ScriptHost) -> Result<Dependent, LoadError> {
        let fail = |problem| LoadError {
            package: name.to_owned(),
            problem: Box::new(problem),
        };
        let manifest: Manifest = dir
            .read_data(&path(PackageDir::MANIFEST))
            .map_err(content)
            .map_err(fail)?;
        let header = manifest.header();
        if header.name != name {
            return Err(fail(LoadProblem::OtherName(header.name.clone())));
        }
        let content = match &manifest {
            Manifest::Avatar(_) => Content::Avatar(Box::new(
                dir.read_data(&path(AVATAR_DATA))
                    .map_err(content)
                    .map_err(fail)?,
            )),
            Manifest::Loadout(_) => {
                let data = dir.read_data(&path(LOADOUT_DATA));
                Content::Loadout(data.map_err(content).map_err(fail)?)
            }
            Manifest::Mode(_) => return Err(fail(LoadProblem::WrongKind)),
        };
        let package = Package::read(dir, name.to_owned(), header.engine, parser)?;
        Ok(Dependent { package, content })
    }
}

fn read_mode_manifest(dir: &PackageDir) -> Result<ModeManifest, LoadError> {
    let fail = |problem| LoadError {
        package: dir.root().display().to_string(),
        problem: Box::new(problem),
    };
    let manifest = dir.read_data(&path(PackageDir::MANIFEST));
    match manifest.map_err(content).map_err(fail)? {
        Manifest::Mode(manifest) => Ok(manifest),
        _ => Err(fail(LoadProblem::WrongKind)),
    }
}

/// One of the engine's paths in a package.
fn path(path: &str) -> PackagePath {
    PackagePath::parse(path).expect("the engine's paths are in the package")
}

fn content(error: ContentError) -> LoadProblem {
    LoadProblem::Content(error)
}
