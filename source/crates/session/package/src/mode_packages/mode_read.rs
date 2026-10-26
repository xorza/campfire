use std::sync::{Arc, OnceLock};

use campfire_common::{MapName, PackageFingerprint};
use campfire_package_format::{ContentError, FileIndex, PackageFiles, PackageFinder, PackagePlace};
use campfire_values::PackagePath;

use crate::error::load_problem::LoadProblem;
use crate::error::{LoadError, PackageRef, StoreError};
use crate::files::manifest::Manifest;
use crate::files::mode_manifest::ModeManifest;
use crate::mode_packages::ModePackages;
use crate::mode_packages::mode_base::ModeBase;
use crate::package_name::PackageName;

/// A mode's files and its manifest, with the files of each package the manifest names, each
/// read once: what loads the mode on any of its maps, as an import checks each map it wrote.
#[derive(Debug)]
pub struct ModeRead {
    files: PackageFiles,
    manifest: ModeManifest,
    dependencies: Vec<(PackageName, PackageFiles)>,
    /// What the mode is on any map, read, parsed and compiled at its first map's load, which
    /// each later map's load shares.
    base: OnceLock<Arc<ModeBase>>,
}

impl ModeRead {
    /// The mode at `place` and each package its manifest names at its path from there, each
    /// read once.
    pub fn at(place: &impl PackagePlace) -> Result<Self, LoadError> {
        let fail = |package| move |error| LoadError::new(package, LoadProblem::Content(error));
        let at = PackageRef::Dir(place.dir().to_owned());
        let files = place.files().map_err(fail(at.clone()))?;
        let manifest = Manifest::read_mode(&files, &at)?;
        let dependencies = manifest
            .dependencies
            .iter()
            .map(|(name, dependency)| {
                let files = place
                    .join(&dependency.path)
                    .files()
                    .map_err(fail(PackageRef::Name(name.clone())))?;
                Ok((name.clone(), files))
            })
            .collect::<Result<Vec<_>, LoadError>>()?;
        Ok(Self {
            files,
            manifest,
            dependencies,
            base: OnceLock::new(),
        })
    }

    /// The mode of the fingerprint `mode` that `finder` holds, and each dependency its manifest
    /// names by the fingerprint of the same place in `dependencies`, as a verifier holds them.
    pub fn found(
        finder: &impl PackageFinder,
        mode: PackageFingerprint,
        dependencies: &[PackageFingerprint],
    ) -> Result<Self, StoreError> {
        let files = Self::find(finder, mode)
            .ok_or(StoreError::UnknownMode)?
            .map_err(StoreError::Load)?;
        let manifest = Manifest::read_mode(&files, &PackageRef::PackageFingerprint(mode))
            .map_err(StoreError::Load)?;
        if manifest.dependencies.len() != dependencies.len() {
            return Err(StoreError::DependencyCount);
        }
        let dependencies = manifest
            .dependencies
            .keys()
            .zip(dependencies)
            .map(|(name, &fingerprint)| {
                let files = Self::find(finder, fingerprint)
                    .ok_or_else(|| StoreError::MissingDependency(name.clone()))?
                    .map_err(StoreError::Load)?;
                Ok((name.clone(), files))
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        Ok(Self {
            files,
            manifest,
            dependencies,
            base: OnceLock::new(),
        })
    }

    /// The mode on its map `map`, or its only map when `map` names none: see `ModePackages::new`.
    /// What the mode is on any map is read, parsed and compiled once, at the first map's load,
    /// and the next only reads its map and runs the load's checks of it.
    pub fn on(&self, map: Option<&MapName>) -> Result<ModePackages, LoadError> {
        let base = if let Some(base) = self.base.get() {
            Arc::clone(base)
        } else {
            let dependencies: Vec<_> = self
                .dependencies
                .iter()
                .map(|(name, files)| (name.clone(), files))
                .collect();
            let base = ModeBase::read(&self.files, self.manifest.clone(), &dependencies)?;
            Arc::clone(self.base.get_or_init(|| Arc::new(base)))
        };
        ModePackages::on_map(base, &self.files, map)
    }

    /// The files of the package of `fingerprint` that `finder` holds, which must still have
    /// that fingerprint; none when it holds none.
    fn find(
        finder: &impl PackageFinder,
        fingerprint: PackageFingerprint,
    ) -> Option<Result<PackageFiles, LoadError>> {
        let fail = |error| {
            LoadError::new(
                PackageRef::PackageFingerprint(fingerprint),
                LoadProblem::Content(error),
            )
        };
        let files = finder.find(fingerprint)?.map_err(fail);
        Some(files.and_then(|files| {
            if files.fingerprint() == fingerprint {
                Ok(files)
            } else {
                let index =
                    PackagePath::parse(FileIndex::PATH).expect("the index's path is a package's");
                Err(fail(ContentError::Changed { path: index }))
            }
        }))
    }
}
