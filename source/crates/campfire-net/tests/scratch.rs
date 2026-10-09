use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// A server's or a client's data directory for a test: `data` in a temporary directory, which
/// the data directory makes its owner's only, as `TempDir`'s own mode may let others in.
#[derive(Debug)]
pub(crate) struct Scratch {
    data: PathBuf,
    /// Removes the directory and all it holds when dropped.
    _root: TempDir,
}

impl Scratch {
    pub(crate) fn new() -> Scratch {
        let root = TempDir::new().unwrap();
        Scratch {
            data: root.path().join("data"),
            _root: root,
        }
    }

    /// The data directory's path, not made until a data directory opens it.
    pub(crate) fn path(&self) -> &Path {
        &self.data
    }
}
