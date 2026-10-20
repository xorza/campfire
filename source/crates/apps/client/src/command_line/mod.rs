use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;

use campfire_common::{DeclaredName, MapName, Seat, TlsKey};
use campfire_mode::{OptionValue, SeatPick};
use campfire_net::{JoinRole, SeatBotFile};
use clap::Parser;
use secp256k1::XOnlyPublicKey;

use crate::command_line::error::{ServerAddressError, ServerBotError, TeamNameError};

mod error;

/// What the command line names: the orders file a bot plays, the player's key file, whether the
/// player plays or watches and the teams it watches, the mode to play and its map, and the server:
/// as its listing gives it, or a local one, each with the data directory.
#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) bot: Option<PathBuf>,
    pub(crate) role: JoinRole,
    pub(crate) watch: Vec<DeclaredName>,
    pub(crate) key: Option<PathBuf>,
    pub(crate) mode: PathBuf,
    pub(crate) map: Option<MapName>,
    pub(crate) server: PlayedServer,
}

/// The server the client plays on.
#[derive(Debug)]
pub(crate) enum PlayedServer {
    /// A server elsewhere, as its listing gives it, and the data directory, if one is named.
    Remote {
        address: SocketAddr,
        tls_key: TlsKey,
        key: XOnlyPublicKey,
        tick_hz: NonZeroU32,
        data: Option<PathBuf>,
    },
    /// A local server, on a thread of the client's process, with the bots of `bots`, the values
    /// `options` picks of the mode's options and `seat_picks` of its seats' choices, its data in
    /// the data directory `data`, which it needs.
    Local {
        bots: Vec<SeatBotFile>,
        options: Vec<OptionValue>,
        seat_picks: Vec<SeatPick>,
        data: PathBuf,
    },
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
    server_bots: Vec<SeatBotFile>,
    /// A value of one of the mode's options for the local server's session, as name=value; an
    /// option not named takes its default
    #[arg(long = "option", value_name = "NAME=VALUE", requires = "local")]
    options: Vec<OptionValue>,
    /// A value of one of the mode's choices of values for a seat of the local server's session,
    /// the player's 0 among them, as seat:choice=value; a seat's choice not named starts unpicked
    #[arg(long = "pick", value_name = "SEAT:CHOICE=VALUE", requires = "local")]
    seat_picks: Vec<SeatPick>,
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

/// What the command line clap read as `line` names.
impl From<CommandLine> for Args {
    fn from(line: CommandLine) -> Self {
        let server = match line.remote {
            Some(remote) => PlayedServer::Remote {
                address: remote.address,
                tls_key: remote.tls_key,
                key: remote.server_key,
                tick_hz: remote.tick_hz,
                data: line.data,
            },
            None => PlayedServer::Local {
                bots: line.server_bots,
                options: line.options,
                seat_picks: line.seat_picks,
                data: line.data.expect("clap takes --local only with --data"),
            },
        };
        let role = if line.spectate {
            JoinRole::Watch
        } else {
            JoinRole::Play
        };
        Self {
            bot: line.bot,
            role,
            watch: line.watch,
            key: line.key,
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
    fn team(text: &str) -> Result<DeclaredName, TeamNameError> {
        DeclaredName::new(text).ok_or(TeamNameError)
    }

    /// The server bot `text` names; an error for a bot in slot 0, which the client plays.
    fn server_bot(text: &str) -> Result<SeatBotFile, ServerBotError> {
        let bot: SeatBotFile = text.parse().map_err(ServerBotError::File)?;
        if bot.seat == Seat::new(0) {
            return Err(ServerBotError::ClientSlot);
        }
        Ok(bot)
    }
}

#[cfg(test)]
mod tests;
