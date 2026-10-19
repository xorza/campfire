use std::num::ParseIntError;

use thiserror::Error;

/// Why text does not name a server's bot.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SeatBotFileError {
    /// The text has no `=` between a seat and a file.
    #[error("{0}: not <seat>=<orders file>")]
    NotPair(String),
    #[error("{text}: not a seat number")]
    Seat {
        text: String,
        #[source]
        error: ParseIntError,
    },
}
