use std::path::PathBuf;

use clap::Parser;

/// Imports the Zero Hour install `GENERALSZH_DATA` names and checks what it gives.
#[derive(Debug, Parser)]
#[command(version)]
pub(crate) struct CommandLine {
    /// The directory the import's packages go to, in place of those an earlier run wrote there
    pub(crate) out: PathBuf,
}
