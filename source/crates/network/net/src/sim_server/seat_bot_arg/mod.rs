use std::path::PathBuf;

use campfire_bots::{OrderScript, OrderScriptReadError};
use campfire_common::{DeclaredName, Seat};

use crate::sim_server::seat_bot_arg::error::SeatBotArgError;
use crate::sim_server::server_bots::{BotPlan, PlannedBot};

pub(crate) mod error;

/// A server's bot as a command line names it: the seat it plays, by its index, and what plays
/// it (08-sessions.md, A mode names its bots).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatBotArg {
    pub seat: Seat,
    pub bot: BotArg,
}

/// What plays a seat of a server's bot: an order script, by its file, or a bot the mode names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BotArg {
    Orders(PathBuf),
    Mode(DeclaredName),
}

impl SeatBotArg {
    /// The seat of an order script, as `<seat>=<orders file>` names it; the first `=` splits it,
    /// so a file's name may hold another.
    pub fn orders(text: &str) -> Result<Self, SeatBotArgError> {
        Self::parse(text, |path| Ok(BotArg::Orders(PathBuf::from(path))))
    }

    /// The seat of a bot of the mode, as `<seat>=<bot>` names it.
    pub fn mode(text: &str) -> Result<Self, SeatBotArgError> {
        Self::parse(text, |name| {
            let name =
                DeclaredName::new(name).ok_or_else(|| SeatBotArgError::Name(name.to_owned()))?;
            Ok(BotArg::Mode(name))
        })
    }

    /// The seat `text` names before its first `=`, and the bot `bot` reads from what follows.
    fn parse(
        text: &str,
        bot: impl FnOnce(&str) -> Result<BotArg, SeatBotArgError>,
    ) -> Result<Self, SeatBotArgError> {
        let (seat, rest) = text
            .split_once('=')
            .ok_or_else(|| SeatBotArgError::NotPair(text.to_owned()))?;
        let seat = seat.parse().map_err(|error| SeatBotArgError::Seat {
            text: seat.to_owned(),
            error,
        })?;
        Ok(Self {
            seat: Seat::new(seat),
            bot: bot(rest)?,
        })
    }

    /// The bot's plan: its order script read from its file, or the mode's bot by its name.
    pub fn read(&self) -> Result<PlannedBot, OrderScriptReadError> {
        let plan = match &self.bot {
            BotArg::Orders(path) => BotPlan::Orders(OrderScript::read(path)?),
            BotArg::Mode(name) => BotPlan::Mode(name.clone()),
        };
        Ok(PlannedBot {
            seat: self.seat,
            plan,
        })
    }
}

#[cfg(test)]
mod tests;
