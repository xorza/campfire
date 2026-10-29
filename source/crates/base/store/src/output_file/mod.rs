use std::io::{self, Write};
use std::path::Path;
use std::process::Stdio;

use campfire_platform::UserOnly;

use crate::durable_dir::DurableDir;
use crate::durable_dir::error::DurableDirError;
use crate::path_error::PathError;

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

    /// Makes the directory `path`, each its user's only, and each one above it that is missing,
    /// with no sync, as the files `write_new` writes take none: a crash may lose them, and the
    /// read that checks the files finds it.
    pub fn create_dirs(path: &Path) -> Result<(), PathError<DurableDirError>> {
        DurableDir::make_all_unsynced(path)
    }
}

#[cfg(test)]
mod tests;
