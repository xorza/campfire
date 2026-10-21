use std::path::PathBuf;

use clap::{Parser, ValueEnum};

/// Imports a game's install into a package of its own, beside its game's rules package, the same
/// bytes on every machine.
#[derive(Debug, Parser)]
#[command(version)]
pub(crate) struct CommandLine {
    /// The game the install holds
    pub(crate) game: Game,
    /// The install's directory, as the game runs from it
    pub(crate) install: PathBuf,
    /// The directory of the packages, which the import makes, and refuses when it is there: the
    /// rules package and the install's, `game/`, the mode to play
    pub(crate) out: PathBuf,
}

/// The games the importer reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Game {
    /// Command & Conquer: Generals – Zero Hour, with base Generals beside it
    ZeroHour,
}

#[cfg(test)]
mod tests;
