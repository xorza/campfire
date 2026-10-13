use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;

use campfire_common::{MapName, PlayerSlot, TlsKey};
use campfire_net::SlotBotFile;
use campfire_values::DeclaredName;
use clap::Parser;
use secp256k1::XOnlyPublicKey;

use crate::args::error::{ServerAddressError, ServerBotError};

mod error;

/// What the command line names: the orders file a bot plays, the player's key file, the data
/// directory, whether the player watches and the teams it watches, the mode to play and its map,
/// and the server: as its listing gives it, or a local one.
#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) bot: Option<PathBuf>,
    pub(crate) spectate: bool,
    pub(crate) watch: Vec<DeclaredName>,
    pub(crate) key: Option<PathBuf>,
    pub(crate) data: Option<PathBuf>,
    pub(crate) mode: PathBuf,
    pub(crate) map: Option<MapName>,
    pub(crate) server: Server,
}

/// The server the client plays on.
#[derive(Debug)]
pub(crate) enum Server {
    /// A server elsewhere, as its listing gives it.
    Remote {
        address: SocketAddr,
        tls_key: TlsKey,
        key: XOnlyPublicKey,
        tick_hz: NonZeroU32,
    },
    /// A local server, on a thread of the client's process, with the bots of `bots`.
    Local { bots: Vec<SlotBotFile> },
}

/// Joins a session on a server, or plays a mode alone on a local server.
#[derive(Debug, Parser)]
#[command(
    version,
    override_usage = "campfire-client [OPTIONS] <MODE> <ADDRESS> <TLS_KEY> <SERVER_KEY> <TICK_HZ>\n       \
                      campfire-client --local --data <DIRECTORY> [OPTIONS] <MODE>"
)]
pub(crate) struct CommandLine {
    /// Plays the orders file's script as a bot, with no window
    #[arg(long, value_name = "ORDERS FILE")]
    bot: Option<PathBuf>,
    /// The player's key file, made when missing; without it, the player is a new key each run
    #[arg(long, value_name = "KEY FILE")]
    key: Option<PathBuf>,
    /// The data directory, which keeps the newest receipt of the session and a local server's data
    #[arg(long, value_name = "DIRECTORY")]
    data: Option<PathBuf>,
    /// Plays the mode alone on a local server, a thread of the client's process
    #[arg(long, requires = "data")]
    local: bool,
    /// Watches the session as a spectator, with no slot
    #[arg(long, conflicts_with_all = ["local", "bot"])]
    spectate: bool,
    /// A team whose view the spectator watches, in a session whose spectators watch teams' views;
    /// every team's together without one
    #[arg(long, value_name = "TEAM", requires = "spectate", value_parser = CommandLine::team)]
    watch: Vec<DeclaredName>,
    /// A bot of the local server, in a slot other than the player's 0
    #[arg(
        long = "server-bot",
        value_name = "SLOT=ORDERS FILE",
        requires = "local",
        value_parser = CommandLine::server_bot
    )]
    server_bots: Vec<SlotBotFile>,
    /// The mode's map the session plays, as the listing names it; the mode's only map when it has
    /// one
    #[arg(long, value_name = "NAME")]
    map: Option<MapName>,
    /// The mode's package directory
    mode: PathBuf,
    #[command(flatten)]
    remote: Option<Remote>,
}

/// A server elsewhere, as its listing gives it: required unless `--local` is given, and refused
/// with it.
#[derive(Debug, clap::Args)]
#[group(conflicts_with = "local")]
struct Remote {
    /// The server's address, as its listing gives it
    #[arg(value_parser = CommandLine::server_address)]
    address: SocketAddr,
    /// The server's TLS key, in hex
    tls_key: TlsKey,
    /// The server's public key, in hex
    server_key: XOnlyPublicKey,
    /// The session's ticks a second
    tick_hz: NonZeroU32,
}

impl Args {
    /// What the command line clap read as `line` names.
    pub(crate) fn of(line: CommandLine) -> Args {
        let server = match line.remote {
            Some(remote) => Server::Remote {
                address: remote.address,
                tls_key: remote.tls_key,
                key: remote.server_key,
                tick_hz: remote.tick_hz,
            },
            None => Server::Local {
                bots: line.server_bots,
            },
        };
        Args {
            bot: line.bot,
            spectate: line.spectate,
            watch: line.watch,
            key: line.key,
            data: line.data,
            mode: line.mode,
            map: line.map,
            server,
        }
    }
}

impl CommandLine {
    /// The server's address `text` names; an error for one that names no host or no port.
    fn server_address(text: &str) -> Result<SocketAddr, ServerAddressError> {
        let address: SocketAddr = text.parse().map_err(ServerAddressError::Syntax)?;
        if address.ip().is_unspecified() {
            return Err(ServerAddressError::Unspecified);
        }
        if address.port() == 0 {
            return Err(ServerAddressError::NoPort);
        }
        Ok(address)
    }

    /// The team `text` names, as the mode's teams are named.
    fn team(text: &str) -> Result<DeclaredName, &'static str> {
        DeclaredName::new(text).ok_or("no team's name")
    }

    /// The server bot `text` names; an error for a bot in slot 0, which the client plays.
    fn server_bot(text: &str) -> Result<SlotBotFile, ServerBotError> {
        let bot: SlotBotFile = text.parse().map_err(ServerBotError::File)?;
        if bot.slot == PlayerSlot::new(0) {
            return Err(ServerBotError::ClientSlot);
        }
        Ok(bot)
    }
}

#[cfg(test)]
mod tests;
