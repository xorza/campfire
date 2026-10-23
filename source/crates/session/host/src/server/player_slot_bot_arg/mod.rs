use std::path::PathBuf;

use campfire_common::{DeclaredName, PlayerSlot};

use crate::error::OrderFileError;
use crate::order_file::OrderFile;
use crate::server::player_slot_bot_arg::error::PlayerSlotBotArgError;
use crate::server::server_bots::{BotPlan, PlannedBot};

pub(crate) mod error;

/// A server's bot as a command line names it: the player slot it plays, by its index, and what
/// plays it (08-sessions.md, A mode names its bots).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSlotBotArg {
    pub player_slot: PlayerSlot,
    pub bot: BotArg,
}

/// What plays a player slot of a server's bot: an order script, by its file, or a bot the mode
/// names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BotArg {
    Orders(OrderFile),
    Mode(DeclaredName),
}

impl PlayerSlotBotArg {
    /// The player slot of an order script, as `<player slot>=<orders file>` names it; the first `=`
    /// splits it, so a file's name may hold another.
    pub fn orders(text: &str) -> Result<Self, PlayerSlotBotArgError> {
        Self::parse(text, |path| {
            Ok(BotArg::Orders(OrderFile::from(PathBuf::from(path))))
        })
    }

    /// The player slot of a bot of the mode, as `<player slot>=<bot>` names it.
    pub fn mode(text: &str) -> Result<Self, PlayerSlotBotArgError> {
        Self::parse(text, |name| {
            let name = DeclaredName::new(name)
                .ok_or_else(|| PlayerSlotBotArgError::Name(name.to_owned()))?;
            Ok(BotArg::Mode(name))
        })
    }

    /// The player slot `text` names before its first `=`, and the bot `bot` reads from what
    /// follows.
    fn parse(
        text: &str,
        bot: impl FnOnce(&str) -> Result<BotArg, PlayerSlotBotArgError>,
    ) -> Result<Self, PlayerSlotBotArgError> {
        let (player_slot, rest) = text
            .split_once('=')
            .ok_or_else(|| PlayerSlotBotArgError::NotPair(text.to_owned()))?;
        let player_slot =
            player_slot
                .parse()
                .map_err(|error| PlayerSlotBotArgError::PlayerSlot {
                    text: player_slot.to_owned(),
                    error,
                })?;
        Ok(Self {
            player_slot: PlayerSlot::new(player_slot),
            bot: bot(rest)?,
        })
    }

    /// The bot's plan: its order script read from its file, or the mode's bot by its name.
    pub fn read(&self) -> Result<PlannedBot, OrderFileError> {
        let plan = match &self.bot {
            BotArg::Orders(file) => BotPlan::Orders(file.read()?),
            BotArg::Mode(name) => BotPlan::Mode(name.clone()),
        };
        Ok(PlannedBot {
            player_slot: self.player_slot,
            plan,
        })
    }
}

#[cfg(test)]
mod tests;
