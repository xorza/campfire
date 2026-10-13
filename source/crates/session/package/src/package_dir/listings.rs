use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use campfire_capabilities::PackagePath;
use campfire_store::{DirEntries, PathError, ReadError};

/// The names on disk of the directories a package's open passes, each directory listed once, as a
/// listing gives each name as it is stored, on every OS. A path holds only when each of its
/// names is in its directory's listing exactly.
#[derive(Debug, Default)]
pub(crate) struct Listings {
    /// Each directory's names, sorted, as `DirEntries` gives them.
    names: BTreeMap<PathBuf, Vec<OsString>>,
}

impl Listings {
    /// Whether each name of `path`, from `root`, is in its directory's listing exactly; not when
    /// a directory on its way is not there.
    pub(crate) fn hold(
        &mut self,
        root: &Path,
        path: &PackagePath,
    ) -> Result<bool, PathError<ReadError>> {
        let mut dir = root.to_owned();
        for name in path.as_str().split('/') {
            let names = self.names_of(&dir)?;
            if names
                .binary_search_by(|held| held.as_os_str().cmp(name.as_ref()))
                .is_err()
            {
                return Ok(false);
            }
            dir.push(name);
        }
        Ok(true)
    }

    /// The names in `dir`, listed the first time the open passes it; none for a directory not
    /// there.
    fn names_of(&mut self, dir: &Path) -> Result<&[OsString], PathError<ReadError>> {
        if !self.names.contains_key(dir) {
            let names = match DirEntries::read(dir) {
                Ok(entries) => entries.into_iter().map(|entry| entry.name).collect(),
                Err(error) if matches!(error.error, ReadError::Missing | ReadError::NotDir) => {
                    Vec::new()
                }
                Err(error) => return Err(error),
            };
            self.names.insert(dir.to_owned(), names);
        }
        Ok(&self.names[dir])
    }
}
