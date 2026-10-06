use std::env;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Play a LAN match and check it, logging into a new directory below `root`.
    Play { root: PathBuf },
    /// Verify the session log of a match played in `dir`, perhaps on another machine, with this
    /// machine's verifier, and compare the server's final hash.
    Verify { dir: PathBuf },
}

/// Plays a LAN match of the real server and clients and checks it, or verifies a run's session
/// log.
#[derive(Debug, Parser)]
#[command(
    version,
    args_conflicts_with_subcommands = true,
    disable_help_subcommand = true
)]
pub(crate) struct CommandLine {
    /// The directory below which each run's directory goes; `campfire-lan-check` in the temporary
    /// directory by default
    root: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

/// The check's subcommands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Verifies the session log of a run, perhaps from another machine, with this machine's
    /// verifier, and compares the server's final hash
    Verify {
        /// The run's directory
        dir: PathBuf,
    },
}

impl Mode {
    /// The mode the command line clap read as `line` names: `[<run root>]`, or `verify <run
    /// directory>`.
    pub(crate) fn of(line: CommandLine) -> Mode {
        match line.command {
            Some(Command::Verify { dir }) => Mode::Verify { dir },
            None => Mode::Play {
                root: line
                    .root
                    .unwrap_or_else(|| env::temp_dir().join("campfire-lan-check")),
            },
        }
    }
}

#[cfg(test)]
mod tests;
