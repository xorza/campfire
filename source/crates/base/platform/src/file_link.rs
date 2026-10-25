use std::path::Path;

use crate::os::Native;

/// A link to a file, which a test makes to see that a walk refuses it: a symbolic link on every
/// OS.
#[derive(Debug)]
pub struct FileLink;

impl FileLink {
    /// Makes `link` a link to the file `target`.
    pub fn make(target: &Path, link: &Path) {
        Native::link_file(target, link).unwrap();
    }

    /// Makes `link` a link to the directory `target`.
    pub fn make_dir(target: &Path, link: &Path) {
        Native::link_dir(target, link).unwrap();
    }
}
