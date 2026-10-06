use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use campfire_net::{OrderScript, OrderScriptReadError, ServerBots, SessionTimes, SlotBotFile};
use clap::Parser;

/// What the command line names: the data directory, how long after its journal's last write a
/// session a stop ended restores, the server's bots, the mode to open, and the address to listen
/// on.
#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) data: PathBuf,
    pub(crate) times: SessionTimes,
    pub(crate) bots: Vec<SlotBotFile>,
    pub(crate) takeover: Option<PathBuf>,
    pub(crate) mode: PathBuf,
    pub(crate) address: SocketAddr,
}

/// Opens a session of a mode on an address, runs its match, and writes its session log once
/// every player left.
#[derive(Debug, Parser)]
#[command(version)]
struct CommandLine {
    /// The data directory, held locked while the server runs: its key, its journal and its logs
    #[arg(long, value_name = "DIRECTORY")]
    data: PathBuf,
    /// How long a session a stop ended restores after its journal's last write
    #[arg(long, value_name = "SECONDS", default_value_t = SessionTimes::DEFAULT.restore_window.as_secs())]
    restore_window: u64,
    /// How long a player whose link failed keeps their slot
    #[arg(long, value_name = "SECONDS", default_value_t = SessionTimes::DEFAULT.grace.as_secs())]
    grace: u64,
    /// A bot of the session, in a slot of its plan
    #[arg(long = "server-bot", value_name = "SLOT=ORDERS FILE")]
    server_bots: Vec<SlotBotFile>,
    /// The orders of the slots that become bots after a leaver, counted from that tick
    #[arg(long, value_name = "ORDERS FILE")]
    takeover: Option<PathBuf>,
    /// The mode's package directory
    mode: PathBuf,
    /// The address to listen on, as 0.0.0.0:4433
    address: SocketAddr,
}

impl Args {
    /// What the command line `args`, the program first, names; clap's error for each flaw, or
    /// for a request for the help or the version.
    pub(crate) fn read(args: impl IntoIterator<Item = OsString>) -> Result<Args, clap::Error> {
        let line = CommandLine::try_parse_from(args)?;
        Ok(Args {
            data: line.data,
            times: SessionTimes {
                grace: Duration::from_secs(line.grace),
                restore_window: Duration::from_secs(line.restore_window),
            },
            bots: line.server_bots,
            takeover: line.takeover,
            mode: line.mode,
            address: line.address,
        })
    }

    /// The server's bots, their scripts read from their files; an error naming a file that does
    /// not read.
    pub(crate) fn server_bots(&self) -> Result<ServerBots, OrderScriptReadError> {
        let slots = self
            .bots
            .iter()
            .map(SlotBotFile::read)
            .collect::<Result<_, _>>()?;
        let takeover = self
            .takeover
            .as_deref()
            .map(OrderScript::read)
            .transpose()?;
        Ok(ServerBots { slots, takeover })
    }
}

#[cfg(test)]
mod tests;
