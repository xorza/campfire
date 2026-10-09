use std::ffi::OsString;
use std::fs::{self, FileType};
use std::io;
use std::path::Path;

use crate::input_file::error::ReadError;
use crate::path_error::PathError;

/// A directory's entries, sorted by their names' bytes, so a walk meets them in the same order
/// on every OS, each with its kind as the entry itself is, a link not followed. No call asks
/// whether a path exists: a caller lists the directory once, and every name it finds comes from
/// one moment.
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
    /// The entries of the directory at `path`; `Missing` when there is none, `NotDir` when the
    /// path names another kind of entry.
    pub fn read(path: &Path) -> Result<Vec<DirEntry>, PathError<ReadError>> {
        DirEntries::read_case(path).map_err(PathError::at(path))
    }

    fn read_case(path: &Path) -> Result<Vec<DirEntry>, ReadError> {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(ReadError::Missing);
            }
            Err(_) if fs::metadata(path).is_ok_and(|metadata| !metadata.is_dir()) => {
                return Err(ReadError::NotDir);
            }
            Err(error) => return Err(ReadError::Read(error)),
        };
        let mut read = Vec::new();
        for entry in entries {
            let entry = entry.map_err(ReadError::Read)?;
            let kind = EntryKind::of(entry.file_type().map_err(ReadError::Read)?);
            read.push(DirEntry {
                name: entry.file_name(),
                kind,
            });
        }
        read.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        Ok(read)
    }
}

impl EntryKind {
    fn of(kind: FileType) -> EntryKind {
        if kind.is_symlink() {
            EntryKind::Link
        } else if kind.is_dir() {
            EntryKind::Dir
        } else if kind.is_file() {
            EntryKind::File
        } else {
            EntryKind::Other
        }
    }
}

#[cfg(test)]
mod tests;
