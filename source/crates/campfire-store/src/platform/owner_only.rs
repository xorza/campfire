use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;

use crate::platform::os::Os;

/// Files and directories only the current user may open, made so and checked: by their owner
/// and mode on Unix, by their owner and ACL on Windows.
#[derive(Debug)]
pub(crate) struct OwnerOnly;

/// Who besides the current user may open a file, as the OS names them: its mode and another
/// owner's id on Unix, the accounts on Windows. Human text, for the person who fixes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exposure(String);

impl OwnerOnly {
    /// A new file at `path`, open for writing; an error when there is one, whose access would
    /// stay what it was.
    pub(crate) fn create_new(path: &Path) -> io::Result<File> {
        Os::create_owner_only(path)
    }

    /// The file at `path`, open for writing and as it is, made when missing.
    pub(crate) fn open_or_create(path: &Path) -> io::Result<File> {
        Os::open_owner_only(path)
    }

    /// Makes the directory `path`, whose parent is there.
    pub(crate) fn create_dir(path: &Path) -> io::Result<()> {
        Os::create_dir_owner_only(path)
    }

    /// Makes the directory `path` and each missing one above it; a directory already there stays
    /// as it is.
    pub(crate) fn create_dir_all(path: &Path) -> io::Result<()> {
        let missing: Vec<&Path> = path
            .ancestors()
            .take_while(|dir| !dir.as_os_str().is_empty() && !dir.is_dir())
            .collect();
        for dir in missing.into_iter().rev() {
            match Os::create_dir_owner_only(dir) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists && dir.is_dir() => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Who else may open `file`, read from its open handle, so the file checked is the file read;
    /// none when only its owner may.
    pub(crate) fn exposure(file: &File) -> io::Result<Option<Exposure>> {
        Ok(Os::exposure(file)?.map(Exposure))
    }

    /// Who else may open the directory at `path`; none when only its owner may.
    pub(crate) fn dir_exposure(path: &Path) -> io::Result<Option<Exposure>> {
        OwnerOnly::exposure(&Os::open_dir(path)?)
    }
}

impl fmt::Display for Exposure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::io;
    use std::path::Path;

    use crate::platform::os::Os;
    use crate::platform::owner_only::{Exposure, OwnerOnly};

    impl OwnerOnly {
        /// Lets other users read the file at `path`: a test's secret file that is not its
        /// owner's only.
        pub(crate) fn expose(path: &Path) {
            Os::expose(path).unwrap();
        }

        /// Who else may open the file or directory at `path`; none when only its owner may.
        pub(crate) fn exposure_at(path: &Path) -> io::Result<Option<Exposure>> {
            Ok(Os::exposure_at(path)?.map(Exposure))
        }
    }
}
