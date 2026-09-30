use std::path::Path;

use campfire_capabilities::{
    HeroData, Manifest, MapData, ModeData, ModeManifest, SpellsData, UnitsData,
};
use campfire_content::{ContentError, PackageDir, PackagePath, PackageStore};
use campfire_protocol::Fingerprint;
use campfire_script::ScriptHost;

use crate::error::{LoadError, LoadProblem, StartError};
use crate::load_check::LoadCheck;
use crate::package::Package;

const MODE_DATA: &str = "data/mode.toml";
const UNITS_DATA: &str = "data/units.toml";
const MAP_DATA: &str = "map/map.toml";
const HERO_DATA: &str = "data/hero.toml";
const SPELLS_DATA: &str = "data/spells.toml";

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
pub(crate) struct Dependent {
    pub(crate) package: Package,
    pub(crate) content: Content,
}

#[derive(Debug)]
pub(crate) enum Content {
    Hero(Box<HeroData>),
    Spells(SpellsData),
}

impl ModePackages {
    /// The mode in `dir`, and its dependencies at the paths its manifest gives, as a workspace
    /// holds them.
    pub fn from_dir(dir: &Path) -> Result<ModePackages, LoadError> {
        let mode = PackageDir::new(dir);
        let manifest = read_mode_manifest(&mode)?;
        let dependencies = manifest
            .dependencies
            .iter()
            .map(|(name, dependency)| (name.clone(), PackageDir::new(dir.join(&dependency.path))))
            .collect::<Vec<_>>();
        ModePackages::assemble(&mode, manifest, &dependencies)
    }

    /// The mode of the fingerprint `mode`, and each dependency its manifest names by the
    /// fingerprint of the same place in `dependencies`, from `store`, as a verifier holds them.
    pub fn from_store(
        store: &PackageStore,
        mode: Fingerprint,
        dependencies: &[Fingerprint],
    ) -> Result<ModePackages, StartError> {
        let dir = store.get(mode).ok_or(StartError::UnknownMode)?.clone();
        let manifest = read_mode_manifest(&dir).map_err(StartError::Load)?;
        if manifest.dependencies.len() != dependencies.len() {
            return Err(StartError::DependencyCount);
        }
        let dependencies = manifest
            .dependencies
            .keys()
            .zip(dependencies)
            .map(|(name, &fingerprint)| {
                let dir = store
                    .get(fingerprint)
                    .ok_or_else(|| StartError::MissingDependency(name.clone()))?;
                Ok((name.clone(), dir.clone()))
            })
            .collect::<Result<Vec<_>, StartError>>()?;
        ModePackages::assemble(&dir, manifest, &dependencies).map_err(StartError::Load)
    }

    pub fn fingerprint(&self) -> Fingerprint {
        self.mode.fingerprint
    }

    /// The fingerprints of its dependencies, in the order of their names in its manifest.
    pub fn dependencies(&self) -> impl ExactSizeIterator<Item = Fingerprint> + '_ {
        self.dependencies
            .iter()
            .map(|dependent| dependent.package.fingerprint)
    }

    pub fn manifest(&self) -> &ModeManifest {
        &self.manifest
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
    /// The package in `dir`, which the mode names `name`: a hero or spells package of that name.
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
            Manifest::Hero(_) => Content::Hero(Box::new(
                dir.read_data(&path(HERO_DATA))
                    .map_err(content)
                    .map_err(fail)?,
            )),
            Manifest::Spells(_) => {
                let data = dir.read_data(&path(SPELLS_DATA));
                Content::Spells(data.map_err(content).map_err(fail)?)
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
