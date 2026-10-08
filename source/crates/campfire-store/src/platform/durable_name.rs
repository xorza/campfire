use std::io;
use std::path::Path;

use crate::platform::os::Os;

/// Names in a directory that survive a crash: a file's new name, and a directory's entries once
/// it gained or lost one.
#[derive(Debug)]
pub(crate) struct DurableName;

impl DurableName {
    /// Gives the file at `from` the name `to` in the same directory, replacing the file of that
    /// name. The name is durable once [`DurableName::sync_dir`] synced the directory.
    pub(crate) fn rename(from: &Path, to: &Path) -> io::Result<()> {
        Os::rename(from, to)
    }

    /// Syncs `directory`, so the names in it survive a crash; the working directory for an
    /// empty path, as a file's parent is when its path has one name.
    pub(crate) fn sync_dir(directory: &Path) -> io::Result<()> {
        if directory.as_os_str().is_empty() {
            Os::sync_dir(Path::new("."))
        } else {
            Os::sync_dir(directory)
        }
    }
}
