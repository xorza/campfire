use std::path::Path;

use crate::platform::os::Os;

/// A link to a file, which a test makes to see that a walk refuses it: a symbolic link on every
/// OS.
#[derive(Debug)]
pub struct FileLink;

impl FileLink {
    /// Makes `link` a link to the file `target`.
    pub fn make(target: &Path, link: &Path) {
        Os::link_file(target, link).unwrap();
    }
}
