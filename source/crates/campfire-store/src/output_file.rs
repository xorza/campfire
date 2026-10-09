use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::process::Stdio;

use crate::path_error::PathError;
use crate::platform::owner_only::OwnerOnly;

/// A new file, written once: one a child process writes to, as a check keeps a process's
/// standard error, made its owner's only and given as the `Stdio` the child takes, so no `File`
/// leaves this crate; or one whose bytes a later check proves, as a package's against its index.
#[derive(Debug)]
pub struct OutputFile;

impl OutputFile {
    /// A new file at `path`, as a child's `Stdio`; an error when one is there.
    pub fn stdio(path: &Path) -> Result<Stdio, PathError<io::Error>> {
        let file = OwnerOnly::create_new(path).map_err(PathError::at(path))?;
        Ok(Stdio::from(file))
    }

    /// A new file at `path` holding `bytes`, with no sync: for a file whose integrity a read
    /// checks, as each file of a package is checked against its index, so a crash that leaves it
    /// short or torn is found where it is read, and thousands of files cost no sync each. An
    /// error when a file is there.
    pub fn write_new(path: &Path, bytes: &[u8]) -> Result<(), PathError<io::Error>> {
        let write = || {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?
                .write_all(bytes)
        };
        write().map_err(PathError::at(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_file::InputFile;
    use crate::scratch::Scratch;

    #[test]
    fn a_new_file_holds_its_bytes_and_never_replaces_one() {
        let scratch = Scratch::new();
        let path = scratch.path("one");
        OutputFile::write_new(&path, b"abc").unwrap();
        assert_eq!(InputFile::read(&path, 3).unwrap(), b"abc");
        let error = OutputFile::write_new(&path, b"x").unwrap_err();
        assert_eq!(error.error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(error.path, path);
        assert_eq!(InputFile::read(&path, 3).unwrap(), b"abc");
        // A missing directory on its way is the error, with the path.
        let deep = scratch.path("no/such");
        assert_eq!(
            OutputFile::write_new(&deep, b"").unwrap_err().error.kind(),
            io::ErrorKind::NotFound
        );
    }
}
