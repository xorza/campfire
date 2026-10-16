use std::path::Path;
use std::sync::Arc;

use campfire_common::MapName;
use campfire_package::{
    ContentError, DependencyPath, LoadError, LoadProblem, ModePackages, PackageFiles, PackageRef,
    PackageSource,
};
use campfire_store::{PathError, ReadError};
use campfire_values::PackagePath;

/// Where a package is: a directory on disk, or, in a test, a directory of a tree in memory. It
/// is the source of the package's files, and the places of the packages its manifest names are
/// its own joined to their paths.
pub(crate) trait PackagePlace: PackageSource + Clone + 'static {
    /// The place of the package at `relative` from this one's, as a manifest names a dependency.
    fn join(&self, relative: &DependencyPath) -> Self;

    /// How an error names the package.
    fn package_ref(&self) -> PackageRef;

    /// The package's files, as the core reads and checks them: see `PackageFiles::read`.
    fn files(&self) -> Result<PackageFiles, ContentError> {
        PackageFiles::read(Arc::new(self.clone()))
    }

    /// The mode at this place on its map `map`, or its only map when `map` names none, with each
    /// package its manifest names at its path from here; each read once.
    fn mode_packages(&self, map: Option<&MapName>) -> Result<ModePackages, LoadError> {
        let read = |place: &Self, package: PackageRef| {
            place
                .files()
                .map_err(|error| LoadError::new(package, LoadProblem::Content(error)))
        };
        let at = self.package_ref();
        let files = read(self, at.clone())?;
        let manifest = files.mode_manifest(&at)?;
        let dependencies = manifest
            .dependencies
            .iter()
            .map(|(name, dependency)| {
                let place = self.join(&dependency.path);
                Ok((name.clone(), read(&place, PackageRef::Name(name.clone()))?))
            })
            .collect::<Result<Vec<_>, LoadError>>()?;
        let dependencies: Vec<_> = dependencies
            .iter()
            .map(|(name, files)| (name.clone(), files))
            .collect();
        ModePackages::new(&files, manifest, map, &dependencies)
    }
}

/// How many bytes a read of a package's file takes: exactly a row's size, or no more than a
/// bound, as the index's own read takes. No read takes a file whole, however long it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadBound {
    Exact(u64),
    AtMost(u64),
}

impl ReadBound {
    /// The length to read of a file of `len` bytes at `path`, `file` where its place holds it:
    /// `Changed` for a length other than an exact bound's, and too large for one past an upper
    /// bound.
    pub(crate) fn take(
        self,
        len: u64,
        path: &PackagePath,
        file: &Path,
    ) -> Result<usize, ContentError> {
        match self {
            Self::Exact(size) if len != size => Err(ContentError::Changed { path: path.clone() }),
            Self::AtMost(max) if len > max => {
                let max = usize::try_from(max).expect("a bound fits usize");
                let error = PathError {
                    path: file.to_owned(),
                    error: ReadError::TooLarge { max },
                };
                Err(ContentError::Read(Box::new(error)))
            }
            _ => Ok(usize::try_from(len).expect("a bounded length fits usize")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_takes_no_more_than_its_bound() {
        let path = PackagePath::parse("textures/x.png").unwrap();
        let file = Path::new("one/textures/x.png");
        // An upper bound takes a file of its length or less, as the index's read takes at most
        // `FileIndex::MAX_LEN`, and refuses one byte more; an exact bound takes its size alone.
        let at_most = ReadBound::AtMost(5);
        assert_eq!(at_most.take(5, &path, file).unwrap(), 5);
        assert_eq!(at_most.take(0, &path, file).unwrap(), 0);
        assert!(matches!(
            at_most.take(6, &path, file),
            Err(ContentError::Read(error)) if error.to_string() == file.display().to_string()
        ));
        let exact = ReadBound::Exact(5);
        assert_eq!(exact.take(5, &path, file).unwrap(), 5);
        for len in [4, 6] {
            assert!(matches!(
                exact.take(len, &path, file),
                Err(ContentError::Changed { path: changed }) if changed == path
            ));
        }
    }
}
