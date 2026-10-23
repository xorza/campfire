use std::num::ParseIntError;

use thiserror::Error;

/// Why text does not name a server's bot.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlayerSlotBotArgError {
    /// The text has no `=` between a player slot and what plays it.
    #[error("{0}: not <player slot>=<bot>")]
    NotPair(String),
    #[error("{text}: not a player slot number")]
    PlayerSlot {
        text: String,
        #[source]
        error: ParseIntError,
    },
    /// The bot's name is no name a mode gives a bot.
    #[error(
        "{0}: not a bot's name, a lowercase letter, then lowercase letters, digits and underscores"
    )]
    Name(String),
}
