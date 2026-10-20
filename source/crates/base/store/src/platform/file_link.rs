use std::path::Path;

use crate::platform::os::Native;

/// A link to a file, which a test makes to see that a walk refuses it: a symbolic link on every
/// OS; and the test of two names of one file, as a hard link gives.
#[derive(Debug)]
pub struct FileLink;

impl FileLink {
    /// Makes `link` a link to the file `target`.
    pub fn make(target: &Path, link: &Path) {
        Native::link_file(target, link).unwrap();
    }

    /// Whether `a` and `b` name one file on disk, as a hard link and its target do: by the
    /// device and inode of each on Unix, the volume and file index on Windows.
    pub fn same(a: &Path, b: &Path) -> bool {
        Native::file_id(a).unwrap() == Native::file_id(b).unwrap()
    }
}
