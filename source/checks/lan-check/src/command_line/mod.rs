use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Args {
    /// Play a LAN match and check it, logging into a new directory below `root`.
    Play { root: PathBuf },
    /// Verify the session log of a match played in `dir`, perhaps on another machine, with this
    /// machine's verifier, and compare the server's final hash: `verifier` when it is given, or
    /// the one beside the check.
    Verify {
        dir: PathBuf,
        verifier: Option<PathBuf>,
    },
}

/// Plays a LAN match of the real server and clients and checks it, or verifies a run's session
/// log.
#[derive(Debug, Parser)]
#[command(
    version,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true,
    disable_help_subcommand = true
)]
pub(crate) struct CommandLine {
    /// The directory below which each run's directory goes
    #[arg(required = true)]
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
        /// The verifier to run; by default, the one beside the check, which `cargo drive run lan`
        /// builds with it
        #[arg(long)]
        verifier: Option<PathBuf>,
    },
}

impl Args {
    /// The arguments the command line clap read as `line` names: `<run root>`, or `verify
    /// [--verifier <verifier>] <run directory>`.
    pub(crate) fn of(line: CommandLine) -> Self {
        match line.command {
            Some(Command::Verify { dir, verifier }) => Self::Verify { dir, verifier },
            None => Self::Play {
                root: line
                    .root
                    .expect("clap requires a run root with no subcommand"),
            },
        }
    }
}

#[cfg(test)]
mod tests;
