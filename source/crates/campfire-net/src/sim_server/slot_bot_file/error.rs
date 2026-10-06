use std::num::ParseIntError;

use thiserror::Error;

/// Why text does not name a server's bot.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SlotBotFileError {
    /// The text has no `=` between a slot and a file.
    #[error("{0}: not <slot>=<orders file>")]
    NotPair(String),
    #[error("{text}: not a slot number")]
    Slot {
        text: String,
        #[source]
        error: ParseIntError,
    },
}
