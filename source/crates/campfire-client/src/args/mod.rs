use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;

use campfire_common::{MapName, PlayerSlot};
use campfire_net::SlotBotFile;
use campfire_protocol::CertificateHash;
use campfire_protocol::secp256k1::XOnlyPublicKey;
use clap::Parser;

use crate::args::error::ServerBotError;

mod error;

/// What the command line names: the orders file a bot plays, the player's key file, the data
/// directory, the mode to play and its map, and the server: as its listing gives it, or a local
/// one.
#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) bot: Option<PathBuf>,
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
        certificate: CertificateHash,
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
    override_usage = "campfire-client [OPTIONS] <MODE> <ADDRESS> <CERTIFICATE> <SERVER_KEY> <TICK_HZ>\n       \
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
    address: SocketAddr,
    /// The hash of the server's TLS certificate, in hex
    certificate: CertificateHash,
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
                certificate: remote.certificate,
                key: remote.server_key,
                tick_hz: remote.tick_hz,
            },
            None => Server::Local {
                bots: line.server_bots,
            },
        };
        Args {
            bot: line.bot,
            key: line.key,
            data: line.data,
            mode: line.mode,
            map: line.map,
            server,
        }
    }
}

impl CommandLine {
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
