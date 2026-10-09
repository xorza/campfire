use std::io;
use std::path::Path;
use std::process::Stdio;

use crate::path_error::PathError;
use crate::platform::owner_only::OwnerOnly;

/// A new file a child process writes to, as a check keeps a process's standard error: made its
/// owner's only, and given as the `Stdio` the child takes, so no `File` leaves this crate.
#[derive(Debug)]
pub struct OutputFile;

impl OutputFile {
    /// A new file at `path`, as a child's `Stdio`; an error when one is there.
    pub fn stdio(path: &Path) -> Result<Stdio, PathError<io::Error>> {
        let file = OwnerOnly::create_new(path).map_err(PathError::at(path))?;
        Ok(Stdio::from(file))
    }
}
