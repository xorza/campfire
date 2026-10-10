use std::env;
use std::path::PathBuf;

use clap::Parser;

/// Imports the Zero Hour install `GENERALSZH_DATA` names and checks what it gives.
#[derive(Debug, Parser)]
#[command(version)]
pub(crate) struct CommandLine {
    /// The directory below which each run's directory goes; `campfire-zero-hour-check` in the
    /// temporary directory by default
    root: Option<PathBuf>,
}

impl CommandLine {
    /// The run root it names, or the default one.
    pub(crate) fn root(self) -> PathBuf {
        self.root
            .unwrap_or_else(|| env::temp_dir().join("campfire-zero-hour-check"))
    }
}
