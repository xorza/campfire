use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;

use crate::platform::os::Native;

/// Files and directories only the current user may open, made so and checked: by their owner
/// and mode on Unix, by their owner and ACL on Windows.
#[derive(Debug)]
pub(crate) struct UserOnly;

/// Who besides the current user may open a file, as the OS names them: its mode and another
/// owner's id on Unix, the accounts on Windows. Human text, for the person who fixes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exposure(String);

impl UserOnly {
    /// A new file at `path`, open for writing; an error when there is one, whose access would
    /// stay what it was.
    pub(crate) fn create_new(path: &Path) -> io::Result<File> {
        Native::create_user_only(path)
    }

    /// The file at `path`, open for writing and as it is, made when missing.
    pub(crate) fn open_or_create(path: &Path) -> io::Result<File> {
        Native::open_user_only(path)
    }

    /// Makes the directory `path`, whose parent is there.
    pub(crate) fn create_dir(path: &Path) -> io::Result<()> {
        Native::create_dir_user_only(path)
    }

    /// Who else may open `file`, read from its open handle, so the file checked is the file read;
    /// none when only its user may.
    pub(crate) fn exposure(file: &File) -> io::Result<Option<Exposure>> {
        Ok(Native::exposure(file)?.map(Exposure))
    }

    /// Who else may open the directory at `path`; none when only its user may.
    pub(crate) fn dir_exposure(path: &Path) -> io::Result<Option<Exposure>> {
        Self::exposure(&Native::open_dir(path)?)
    }
}

impl fmt::Display for Exposure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::io;
    use std::path::Path;

    use crate::platform::os::Native;
    use crate::platform::user_only::{Exposure, UserOnly};

    impl UserOnly {
        /// Lets other users read the file at `path`: a test's secret file that is not its
        /// user's only.
        pub(crate) fn expose(path: &Path) {
            Native::expose(path).unwrap();
        }

        /// Who else may open the file or directory at `path`; none when only its user may.
        pub(crate) fn exposure_at(path: &Path) -> io::Result<Option<Exposure>> {
            Ok(Native::exposure_at(path)?.map(Exposure))
        }
    }
}
