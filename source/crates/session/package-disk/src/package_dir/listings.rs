use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use campfire_store::{DirEntries, DirEntry, EntryKind, PathError, ReadError};
use campfire_values::PackagePath;

/// The entries on disk of the directories a package's open passes, each directory listed once, as
/// a listing gives each name as it is stored, on every OS. A path holds only when each of its
/// names is in its directory's listing exactly, a directory on its way and a file at its end.
#[derive(Debug, Default)]
pub(crate) struct Listings {
    /// Each directory's entries, sorted by name, as `DirEntries` gives them.
    entries: BTreeMap<PathBuf, Vec<DirEntry>>,
}

impl Listings {
    /// Whether `path`, from `root`, is a file whose names are each in its directory's listing
    /// exactly, the ones before its last a directory's; not when a directory on its way is not
    /// there, or a link, or its end is not a file, as a row reads its bytes from a file alone.
    pub(crate) fn hold(
        &mut self,
        root: &Path,
        path: &PackagePath,
    ) -> Result<bool, PathError<ReadError>> {
        let mut dir = root.to_owned();
        let mut names = path.as_str().split('/').peekable();
        while let Some(name) = names.next() {
            let entries = self.entries_of(&dir)?;
            let Ok(at) =
                entries.binary_search_by(|entry| entry.name.as_os_str().cmp(name.as_ref()))
            else {
                return Ok(false);
            };
            let kind = if names.peek().is_some() {
                EntryKind::Dir
            } else {
                EntryKind::File
            };
            if entries[at].kind != kind {
                return Ok(false);
            }
            dir.push(name);
        }
        Ok(true)
    }

    /// The entries in `dir`, listed the first time the open passes it; none for a directory not
    /// there.
    fn entries_of(&mut self, dir: &Path) -> Result<&[DirEntry], PathError<ReadError>> {
        if !self.entries.contains_key(dir) {
            let entries = match DirEntries::read(dir) {
                Ok(entries) => entries,
                Err(error) if matches!(error.error, ReadError::Missing | ReadError::NotDir) => {
                    Vec::new()
                }
                Err(error) => return Err(error),
            };
            self.entries.insert(dir.to_owned(), entries);
        }
        Ok(&self.entries[dir])
    }
}
