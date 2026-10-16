use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_codec::Toml;
use campfire_common::PackageFingerprint;
use campfire_values::PackagePath;
use serde::de::DeserializeOwned;

use crate::error::load_problem::LoadProblem;
use crate::error::{ContentError, LoadError, PackageRef};
use crate::file_index::FileIndex;
use crate::files::manifest::Manifest;
use crate::files::mode_manifest::ModeManifest;
use crate::package_reader::PackageReader;
use crate::package_source::PackageSource;

/// The directory of a package's scripts.
pub(crate) const SCRIPTS: &str = "scripts";

/// The directory of a package's locales.
pub(crate) const LOCALE: &str = "locale";

/// The directories whose files a load reads, beside the manifest.
const READ_DIRS: [&str; 3] = ["data", SCRIPTS, LOCALE];

/// A package as a load reads it: the files a load reads, read once into memory and checked
/// against their rows, each file's bytes by its path in the package, and its reader. A load
/// parses only these bytes, so what it parses is what the fingerprint names; any other file it
/// reads on demand through the reader, checked the same way.
#[derive(Debug, Clone)]
pub struct PackageFiles {
    /// In the order of their paths.
    files: BTreeMap<PackagePath, Vec<u8>>,
    reader: PackageReader,
}

impl PackageFiles {
    /// The file every package has at its root.
    pub const MANIFEST: &str = "manifest.toml";

    /// The package of `source`: its index, and the files a load reads, the manifest's and those
    /// under `data/`, `scripts/` and `locale/`, each read once into memory and checked against
    /// its row. Any other file, as a session's map, reads on demand; no file the index does not
    /// list reads. The open checks that the source holds every file the index lists by its exact
    /// name, and fails on the first it does not (`PackageSource::check_names`). Nothing changes
    /// an open package, so a read after the open takes the path as it is.
    pub fn read(source: Arc<dyn PackageSource>) -> Result<Self, ContentError> {
        let index = FileIndex::decode(&source.read_index()?)?;
        source.check_names(&mut index.rows().map(|(path, _)| path))?;
        let reader = PackageReader::new(index, source);
        let mut files = BTreeMap::new();
        for (path, row) in reader.index().rows() {
            if Self::reads(path) {
                files.insert(path.clone(), reader.read_row(path, row)?);
            }
        }
        Ok(Self { files, reader })
    }

    /// One of the engine's paths in a package, as a package names its files.
    pub(crate) fn engine_path(path: &str) -> PackagePath {
        PackagePath::parse(path).expect("the engine's paths are in the package")
    }

    /// Whether a load reads the file at `path`: the manifest, and the files of its data, its
    /// scripts and its locales.
    pub(crate) fn reads(path: &PackagePath) -> bool {
        path.as_str() == Self::MANIFEST || READ_DIRS.iter().any(|dir| path.is_under(dir))
    }

    pub const fn fingerprint(&self) -> PackageFingerprint {
        self.reader.fingerprint()
    }

    /// Its reader of any file on demand, which its package keeps.
    pub const fn reader(&self) -> &PackageReader {
        &self.reader
    }

    /// The files under the directory `dir` that a load reads, in the order of their paths.
    pub fn files_under<'a>(&'a self, dir: &'a str) -> impl Iterator<Item = &'a PackagePath> {
        self.files.keys().filter(move |path| path.is_under(dir))
    }

    /// The TOML data file at `path`, read as a `T`.
    pub fn read_data<T: DeserializeOwned>(&self, path: &PackagePath) -> Result<T, ContentError> {
        Toml::parse(self.read_text(path)?).map_err(|error| ContentError::Data {
            path: path.clone(),
            error,
        })
    }

    /// The text of the file at `path` that a load reads, such as a script's source.
    pub fn read_text(&self, path: &PackagePath) -> Result<&str, ContentError> {
        let bytes = self
            .files
            .get(path)
            .ok_or_else(|| ContentError::Missing { path: path.clone() })?;
        Self::text(path, bytes)
    }

    pub(crate) const fn index(&self) -> &FileIndex {
        self.reader.index()
    }

    fn text<'a>(path: &PackagePath, bytes: &'a [u8]) -> Result<&'a str, ContentError> {
        str::from_utf8(bytes).map_err(|error| ContentError::NotText {
            path: path.clone(),
            error,
        })
    }

    /// The bytes of any file of the package, read now and checked against its row.
    pub fn read_file(&self, path: &PackagePath) -> Result<Vec<u8>, ContentError> {
        self.reader.read_file(path)
    }

    /// Its manifest, a mode's, as the package `package`; an error for a manifest that does not
    /// read, or of another kind of package.
    pub fn mode_manifest(&self, package: &PackageRef) -> Result<ModeManifest, LoadError> {
        let fail = |problem| LoadError::new(package.clone(), problem);
        let manifest = self.read_data(&Self::engine_path(Self::MANIFEST));
        match manifest.map_err(LoadProblem::Content).map_err(fail)? {
            Manifest::Mode(manifest) => manifest
                .checked()
                .map_err(|error| fail(LoadProblem::Capabilities(error))),
            _ => Err(fail(LoadProblem::WrongKind)),
        }
    }
}
