use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Stdio;

use crate::path_error::PathError;
use campfire_platform::UserOnly;

/// A new file, written once, its user's only, as every file `store` makes: one a child process
/// writes to, as a check keeps a process's standard error, given as the `Stdio` the child takes,
/// so no `File` leaves this crate; or one whose bytes a later check proves, as a package's
/// against its index.
#[derive(Debug)]
pub struct OutputFile;

impl OutputFile {
    /// A new file at `path`, as a child's `Stdio`; an error when one is there.
    pub fn stdio(path: &Path) -> Result<Stdio, PathError<io::Error>> {
        let file = UserOnly::create_new(path).map_err(PathError::at(path))?;
        Ok(Stdio::from(file))
    }

    /// A new file at `path` holding `bytes`, with no sync: for a file whose integrity a read
    /// checks, as each file of a package is checked against its index, so a crash that leaves it
    /// short or torn is found where it is read, and thousands of files cost no sync each. An
    /// error when a file is there.
    pub fn write_new(path: &Path, bytes: &[u8]) -> Result<(), PathError<io::Error>> {
        let write = || UserOnly::create_new(path)?.write_all(bytes);
        write().map_err(PathError::at(path))
    }

    /// A new file at `path` that is the file `same_as`, which holds `bytes`: a hard link to it,
    /// so both names are one file on disk, as a package keeps one copy of identical files; or,
    /// where the file system makes no such link, as of no links, a full count of them or another
    /// device, a new file of `bytes`, as `write_new` writes it. An error when a file is there.
    pub fn link_new(path: &Path, same_as: &Path, bytes: &[u8]) -> Result<(), PathError<io::Error>> {
        match fs::hard_link(same_as, path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Err(PathError::at(path)(error))
            }
            Err(_) => Self::write_new(path, bytes),
        }
    }
}

#[cfg(test)]
mod tests;
