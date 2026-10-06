use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;

use campfire_common::PlayerSlot;
use campfire_net::SlotBotFile;
use campfire_protocol::CertificateHash;
use campfire_protocol::secp256k1::XOnlyPublicKey;
use clap::Parser;

use crate::args::error::ArgsError;

pub(crate) mod error;

/// What the command line names: the orders file a bot plays, the player's key file, the data
/// directory, the mode to play, and the server: as its listing gives it, or a local one.
#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) bot: Option<PathBuf>,
    pub(crate) key: Option<PathBuf>,
    pub(crate) data: Option<PathBuf>,
    pub(crate) mode: PathBuf,
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
#[command(version)]
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
        requires = "local"
    )]
    server_bots: Vec<SlotBotFile>,
    /// The mode's package directory
    mode: PathBuf,
    /// The server's address, as its listing gives it
    #[arg(required_unless_present = "local", conflicts_with = "local")]
    address: Option<SocketAddr>,
    /// The hash of the server's TLS certificate, in hex
    #[arg(required_unless_present = "local", conflicts_with = "local")]
    certificate: Option<CertificateHash>,
    /// The server's public key, in hex
    #[arg(required_unless_present = "local", conflicts_with = "local")]
    server_key: Option<XOnlyPublicKey>,
    /// The session's ticks a second
    #[arg(required_unless_present = "local", conflicts_with = "local")]
    tick_hz: Option<NonZeroU32>,
}

impl Args {
    /// What the command line clap read as `line` names; an error for a flaw clap cannot state.
    pub(crate) fn of(line: CommandLine) -> Result<Args, ArgsError> {
        let server = if line.local {
            if line
                .server_bots
                .iter()
                .any(|bot| bot.slot == PlayerSlot::new(0))
            {
                return Err(ArgsError::BotInClientSlot);
            }
            Server::Local {
                bots: line.server_bots,
            }
        } else {
            let (Some(address), Some(certificate), Some(key), Some(tick_hz)) = (
                line.address,
                line.certificate,
                line.server_key,
                line.tick_hz,
            ) else {
                unreachable!("clap requires a remote server's four arguments without --local");
            };
            Server::Remote {
                address,
                certificate,
                key,
                tick_hz,
            }
        };
        Ok(Args {
            bot: line.bot,
            key: line.key,
            data: line.data,
            mode: line.mode,
            server,
        })
    }
}

#[cfg(test)]
mod tests;
