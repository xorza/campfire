use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::{self, Path, PathBuf};

use campfire_common::{DeclaredName, MapName, PlayerSlot, Tick};
use campfire_host::{JoinRole, OrderFile, PlayerSlotBotArg, ReportCamera, ReportDir};
use campfire_mode::{OptionValue, PlayerSlotPick};
use campfire_net_core::TlsKey;
use campfire_package::ModePackages;
use campfire_values::TeamSet;
use clap::{Parser, ValueEnum};
use secp256k1::XOnlyPublicKey;

use crate::command_line::error::{ArgsError, ServerAddressError, TeamNameError, UnknownTeam};

pub(crate) mod error;

/// What the command line names: the orders file a bot plays, the player's key file, whether the
/// player plays or watches and the teams it watches, the mode to play and its map, and the server:
/// as its listing gives it, a local one, or a local one that replays a report, each with the data
/// directory; a report names the mode, the map and the role itself.
#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) bot: Option<OrderFile>,
    pub(crate) role: RoleArg,
    pub(crate) key: Option<PathBuf>,
    pub(crate) mode: PathBuf,
    pub(crate) map: Option<MapName>,
    pub(crate) server: PlayedServer,
    pub(crate) console: Option<ConsoleArg>,
}

/// Where a game's console reads its requests and writes its replies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ConsoleArg {
    /// Its standard input and output, a JSON line each, its log staying on standard error.
    Stdio,
}

/// Whether the player plays or watches, with the names of the teams a spectator watches, which
/// the mode's packages resolve.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RoleArg {
    Play,
    Watch { teams: Vec<DeclaredName> },
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
    /// `options` picks of the mode's options and `player_slot_picks` of its player slots' choices,
    /// its data in the data directory `data`, which it needs.
    Local {
        bots: Vec<PlayerSlotBotArg>,
        options: Vec<OptionValue>,
        player_slot_picks: Vec<PlayerSlotPick>,
        data: PathBuf,
    },
    /// A local server that replays the report `report` through the tick `through`, its client
    /// watching with its camera where the report's stood, `camera`, its data in the data
    /// directory `data`, whose server's key signed the report's log (08-sessions.md, The client
    /// opens a report).
    Replay {
        report: ReportDir,
        through: Tick,
        camera: ReportCamera,
        data: PathBuf,
    },
}

/// Joins a session on a server, or plays a mode alone on a local server.
#[derive(Debug, Parser)]
#[command(
    version,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true,
    override_usage = "campfire-client [OPTIONS] <MODE> <ADDRESS> <TLS_KEY> <SERVER_KEY> <TICK_HZ>\n       \
                      campfire-client --local --data <DIRECTORY> [OPTIONS] <MODE>\n       \
                      campfire-client report [--data <DIRECTORY>] <REPORT>"
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
    /// Watches the session as a spectator, with no player slot; of a local server, which then
    /// admits it as its one spectator, who sees every unit
    #[arg(long, conflicts_with = "bot")]
    spectate: bool,
    /// A team whose view the spectator watches, in a session whose spectators watch teams' views;
    /// every team's together without one. A local server's spectator sees every unit
    #[arg(
        long,
        value_name = "TEAM",
        requires = "spectate",
        conflicts_with = "local",
        value_parser = CommandLine::team
    )]
    watch: Vec<DeclaredName>,
    /// A bot of the mode, by its name, on the local server, in a player slot other than the
    /// player's 0, or in any player slot of a client that watches
    #[arg(
        long = "server-bot",
        value_name = "PLAYER_SLOT=BOT",
        requires = "local",
        value_parser = PlayerSlotBotArg::mode
    )]
    server_bots: Vec<PlayerSlotBotArg>,
    /// The order script of a bot of the local server, its orders at their ticks, in a player slot
    /// as `--server-bot` takes one
    #[arg(
        long = "server-orders",
        value_name = "PLAYER_SLOT=ORDERS FILE",
        requires = "local",
        value_parser = PlayerSlotBotArg::orders
    )]
    server_orders: Vec<PlayerSlotBotArg>,
    /// A value of one of the mode's options for the local server's session, as name=value; an
    /// option not named takes its default
    #[arg(long = "option", value_name = "NAME=VALUE", requires = "local")]
    options: Vec<OptionValue>,
    /// A value of one of the mode's choices of values for a player slot of the local server's
    /// session, the player's 0 among them, as player_slot:choice=value; a player slot's choice not
    /// named starts unpicked
    #[arg(
        long = "pick",
        value_name = "PLAYER_SLOT:CHOICE=VALUE",
        requires = "local"
    )]
    player_slot_picks: Vec<PlayerSlotPick>,
    /// The mode's map the session plays, as the listing names it; the mode's only map when it has
    /// one
    #[arg(long, value_name = "NAME")]
    map: Option<MapName>,
    /// Answer a tool's requests over a console, as the tool that started the client asks; a local
    /// match starts paused after its first tick, and the end of the console's input stops the
    /// client
    #[arg(long, value_name = "STREAMS")]
    console: Option<ConsoleArg>,
    /// The mode's package directory
    #[arg(required = true)]
    mode: Option<PathBuf>,
    #[command(flatten)]
    remote: Option<Remote>,
    #[command(subcommand)]
    open: Option<Open>,
}

/// What the client opens besides a session it joins or plays.
#[derive(Debug, clap::Subcommand)]
enum Open {
    /// Opens a report the client wrote: a local server replays its log through its tick and holds
    /// there paused, the client watching every unit with its camera where the report's stood; the
    /// mode and the map are the report's (08-sessions.md, The client opens a report)
    Report {
        /// The data directory, whose server's key signed the report's log; the one that holds
        /// the report when none is given
        #[arg(long, value_name = "DIRECTORY")]
        data: Option<PathBuf>,
        /// The report's folder, as the client wrote it in its data directory's `reports`
        report: PathBuf,
    },
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

/// What the command line clap read as `line` names; an error for a server bot in player slot 0 of a
/// client that plays it.
impl TryFrom<CommandLine> for Args {
    type Error = ArgsError;

    fn try_from(line: CommandLine) -> Result<Self, ArgsError> {
        if let Some(Open::Report { data, report }) = line.open {
            return Self::of_report(&report, data);
        }
        let role = if line.spectate {
            RoleArg::Watch { teams: line.watch }
        } else {
            RoleArg::Play
        };
        let bots: Vec<_> = line
            .server_bots
            .into_iter()
            .chain(line.server_orders)
            .collect();
        if role == RoleArg::Play && bots.iter().any(|bot| bot.player_slot == PlayerSlot::new(0)) {
            return Err(ArgsError::ClientPlayerSlot);
        }
        let server = match line.remote {
            Some(remote) => PlayedServer::Remote {
                address: remote.address,
                tls_key: remote.tls_key,
                key: remote.server_key,
                tick_hz: remote.tick_hz,
                data: line.data,
            },
            None => PlayedServer::Local {
                bots,
                options: line.options,
                player_slot_picks: line.player_slot_picks,
                data: line.data.expect("clap takes --local only with --data"),
            },
        };
        Ok(Self {
            bot: line.bot.map(OrderFile::from),
            role,
            key: line.key,
            mode: line
                .mode
                .expect("clap takes a mode unless the line opens a report"),
            map: line.map,
            server,
            console: line.console,
        })
    }
}

impl Args {
    /// What a line that opens the report `report` names: the report's mode and map, its client
    /// watching as a new key, its data directory `data`, or the one that holds the report, two
    /// folders up, as a client writes its reports into its data directory's `reports`. An error
    /// for a report whose path does not resolve, whose facts do not read, or that no data
    /// directory holds.
    fn of_report(report: &Path, data: Option<PathBuf>) -> Result<Self, ArgsError> {
        let report = path::absolute(report).map_err(ArgsError::ReportPath)?;
        let held = report.parent().and_then(Path::parent).map(Path::to_owned);
        let data = data.or(held).ok_or(ArgsError::ReportData)?;
        let report = ReportDir::at(report);
        let facts = report.read_facts().map_err(ArgsError::Report)?;
        Ok(Self {
            bot: None,
            role: RoleArg::Watch { teams: Vec::new() },
            key: None,
            mode: facts.mode_dir,
            map: Some(facts.map),
            server: PlayedServer::Replay {
                report,
                through: facts.tick,
                camera: facts.camera,
                data,
            },
            console: None,
        })
    }
}

impl PlayedServer {
    /// Where the window's camera starts: a replayed report's, else where the package's camera
    /// file starts it.
    pub(crate) const fn camera(&self) -> Option<&ReportCamera> {
        match self {
            Self::Replay { camera, .. } => Some(camera),
            Self::Remote { .. } | Self::Local { .. } => None,
        }
    }
}

impl RoleArg {
    /// The role of a join to a session of the mode `packages` holds: a spectator watches the
    /// teams it names, every team's when it names none; an error for a name no team of the mode
    /// has.
    pub(crate) fn resolve(&self, packages: &ModePackages) -> Result<JoinRole, UnknownTeam> {
        let names = match self {
            Self::Play => return Ok(JoinRole::Play),
            Self::Watch { teams } if teams.is_empty() => {
                return Ok(JoinRole::Watch {
                    teams: TeamSet::ALL,
                });
            }
            Self::Watch { teams } => teams,
        };
        let manifest = packages.manifest();
        let watched = names.iter().map(|name| {
            manifest
                .team_named(name.as_str())
                .ok_or_else(|| UnknownTeam(name.clone()))
        });
        Ok(JoinRole::Watch {
            teams: watched.collect::<Result<_, _>>()?,
        })
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
}

#[cfg(test)]
mod tests;
