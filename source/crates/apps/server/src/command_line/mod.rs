use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use campfire_bots::OrderScript;
use campfire_common::MapName;
use campfire_host::{OrderFile, OrderFileError, PlannedBot, PlayerSlotBotArg, SessionTimes};
use campfire_mode::{OptionValue, PlayerSlotPick};
use campfire_visibility::{HostVisibility, SettingValue};
use clap::Parser;

/// What the command line names: the data directory, how long after its journal's last write a
/// session a stop ended restores, the server's bots, the mode to open and its map, what the host
/// sets of a new session's visibility and picks of its mode's options and of its player slots'
/// choices, the address to listen on, and whether the server asks its router to forward the port.
#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) data: PathBuf,
    pub(crate) times: SessionTimes,
    pub(crate) bots: Vec<PlayerSlotBotArg>,
    pub(crate) takeover: Option<OrderFile>,
    pub(crate) mode: PathBuf,
    pub(crate) map: Option<MapName>,
    pub(crate) host: HostVisibility,
    pub(crate) options: Vec<OptionValue>,
    pub(crate) player_slot_picks: Vec<PlayerSlotPick>,
    pub(crate) address: SocketAddr,
    pub(crate) port_map: bool,
}

/// Opens a session of a mode on an address, runs its match, and writes its session log once the
/// session ends.
#[derive(Debug, Parser)]
#[command(version)]
pub(crate) struct CommandLine {
    /// The data directory, held locked while the server runs: its key, its journal and its logs
    #[arg(long, value_name = "DIRECTORY")]
    data: PathBuf,
    /// How long a session a stop ended restores after its journal's last write
    #[arg(long, value_name = "SECONDS", default_value_t = SessionTimes::DEFAULT.restore_window.as_secs())]
    restore_window: u64,
    /// How long a player whose link failed keeps their player_slot
    #[arg(long, value_name = "SECONDS", default_value_t = SessionTimes::DEFAULT.grace.as_secs())]
    grace: u64,
    /// A bot of the mode, by its name, in a player slot of the session's plan
    #[arg(long = "server-bot", value_name = "PLAYER_SLOT=BOT", value_parser = PlayerSlotBotArg::mode)]
    server_bots: Vec<PlayerSlotBotArg>,
    /// The order script of a bot in a player slot of the session's plan, its orders at their ticks
    #[arg(long = "server-orders", value_name = "PLAYER_SLOT=ORDERS FILE", value_parser = PlayerSlotBotArg::orders)]
    server_orders: Vec<PlayerSlotBotArg>,
    /// The orders of the player slots that become bots after a leaver, counted from that tick
    #[arg(long, value_name = "ORDERS FILE")]
    takeover: Option<PathBuf>,
    /// The mode's map the session plays; the mode's only map when it has one
    #[arg(long, value_name = "NAME")]
    map: Option<MapName>,
    /// A visibility setting the mode leaves to the host, as name=value: a kind's recipients that a
    /// mode may set, separated by commas (allies, enemies, spectators); spectators=all or teams;
    /// out=team or spectators
    #[arg(long = "visibility", value_name = "SETTING=VALUE")]
    visibility: Vec<SettingValue>,
    /// A value of one of the mode's options for a new session, as name=value; an option not
    /// named takes its default
    #[arg(long = "option", value_name = "NAME=VALUE")]
    options: Vec<OptionValue>,
    /// A value of one of the mode's choices of values for a player slot of a new session, as
    /// player slot:choice=value; a player slot's choice not named starts unpicked
    #[arg(long = "pick", value_name = "PLAYER_SLOT:CHOICE=VALUE")]
    player_slot_picks: Vec<PlayerSlotPick>,
    /// The most links with no player slot a new session admits, none unless given; a restore checks
    /// one given, 0 too, against the waiting session's
    #[arg(long, value_name = "COUNT")]
    spectators: Option<u32>,
    /// Ask no router to forward the port; a player outside then joins through a port the host
    /// forwards by hand
    #[arg(long)]
    no_port_map: bool,
    /// The mode's package directory
    mode: PathBuf,
    /// The address to listen on, as 0.0.0.0:4433
    address: SocketAddr,
}

/// What the command line clap read as `line` names.
impl From<CommandLine> for Args {
    fn from(line: CommandLine) -> Self {
        Self {
            data: line.data,
            times: SessionTimes {
                grace: Duration::from_secs(line.grace),
                restore_window: Duration::from_secs(line.restore_window),
            },
            bots: line
                .server_bots
                .into_iter()
                .chain(line.server_orders)
                .collect(),
            takeover: line.takeover.map(OrderFile::from),
            mode: line.mode,
            map: line.map,
            host: HostVisibility {
                settings: line.visibility,
                max_spectators: line.spectators,
            },
            options: line.options,
            player_slot_picks: line.player_slot_picks,
            address: line.address,
            port_map: !line.no_port_map,
        }
    }
}

/// The server's bots as the command line plans them, their order scripts read from their files,
/// their names not resolved yet: one for each player slot the command line gives a bot, and the
/// takeover script.
#[derive(Debug)]
pub(crate) struct BotScripts {
    pub(crate) player_slots: Vec<PlannedBot>,
    pub(crate) takeover: Option<OrderScript>,
}

impl Args {
    /// The server's bots, their order scripts read from their files; an error naming a file
    /// that does not read.
    pub(crate) fn bot_scripts(&self) -> Result<BotScripts, OrderFileError> {
        let player_slots = self
            .bots
            .iter()
            .map(PlayerSlotBotArg::read)
            .collect::<Result<_, _>>()?;
        let takeover = self.takeover.as_ref().map(OrderFile::read).transpose()?;
        Ok(BotScripts {
            player_slots,
            takeover,
        })
    }
}

#[cfg(test)]
mod tests;
