mod bots;
mod checkpoints;
mod fog;
mod lane;
mod prototype;
mod receipts;
mod rejoin;
mod restore;
mod scenario;

use std::path::PathBuf;
use std::{fs, process};

/// A directory of its own under the target's temporary directory, removed when dropped.
#[derive(Debug)]
pub(crate) struct Scratch(pub(crate) PathBuf);

impl Scratch {
    pub(crate) fn new(name: &str) -> Scratch {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("restore-{name}-{}", process::id()));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}
