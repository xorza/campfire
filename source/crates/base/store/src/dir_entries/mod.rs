use std::ffi::OsString;
use std::fs::{self, FileType};
use std::io;
use std::path::{Path, PathBuf};

use crate::durable_dir::DurableDir;
use crate::input_file::error::ReadError;
use crate::path_error::PathError;

/// A directory's entries, sorted by their names' bytes, so a walk meets them in the same order
/// on every OS, each with its kind as the entry itself is, a link not followed. No call asks
/// whether a path exists: a caller lists the directory once, and every name it finds comes from
/// one moment. A directory that `DurableDir::remove` is removing is gone from a listing.
#[derive(Debug)]
pub struct DirEntries;

/// An entry of a directory: its name and its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: OsString,
    pub kind: EntryKind,
}

/// What an entry is, a link as itself, not what it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Link,
    /// A device, a pipe, a socket or another entry that is none of the above.
    Other,
}

impl DirEntries {
    /// The path of the file or directory the link at `link` leads to, through every link on the
    /// way, for a caller that means a link's target, as a read follows none; `Missing` when it
    /// leads nowhere.
    pub fn link_target(link: &Path) -> Result<PathBuf, PathError<ReadError>> {
        fs::canonicalize(link)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    ReadError::Missing
                } else {
                    ReadError::Read(error)
                }
            })
            .map_err(PathError::at(link))
    }

    /// The entries of the directory at `path`; `Missing` when there is none, `Link` when its last
    /// name is a link, which is not followed, as a read follows none, `NotDir` when the path names
    /// another kind of entry. A directory swapped for a link after the check is listed through it,
    /// as std opens no directory without following.
    pub fn read(path: &Path) -> Result<Vec<DirEntry>, PathError<ReadError>> {
        Self::read_case(path).map_err(PathError::at(path))
    }

    fn read_case(path: &Path) -> Result<Vec<DirEntry>, ReadError> {
        let missing = |error: io::Error| {
            if error.kind() == io::ErrorKind::NotFound {
                ReadError::Missing
            } else {
                ReadError::Read(error)
            }
        };
        let named = fs::symlink_metadata(path).map_err(missing)?;
        if named.is_symlink() {
            return Err(ReadError::Link);
        }
        if !named.is_dir() {
            return Err(ReadError::NotDir);
        }
        let entries = fs::read_dir(path).map_err(missing)?;
        let mut read = Vec::new();
        for entry in entries {
            let entry = entry.map_err(ReadError::Read)?;
            let name = entry.file_name();
            if DurableDir::is_tombstone(&name) {
                continue;
            }
            let kind = EntryKind::of(entry.file_type().map_err(ReadError::Read)?);
            read.push(DirEntry { name, kind });
        }
        read.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        Ok(read)
    }
}

impl EntryKind {
    #[expect(
        clippy::filetype_is_file,
        reason = "the kind names a regular file, and every other kind is `Other`"
    )]
    pub(crate) fn of(kind: FileType) -> Self {
        if kind.is_symlink() {
            Self::Link
        } else if kind.is_dir() {
            Self::Dir
        } else if kind.is_file() {
            Self::File
        } else {
            Self::Other
        }
    }
}

#[cfg(test)]
mod tests;
