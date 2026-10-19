use std::path::PathBuf;
use std::str::FromStr;

use campfire_bots::{OrderScript, OrderScriptReadError};
use campfire_common::Seat;

use crate::sim_server::seat_bot_file::error::SeatBotFileError;
use crate::sim_server::server_bots::SeatScript;

pub(crate) mod error;

/// A server's bot as a command line names it, `<seat>=<orders file>`: the seat it plays, by its
/// index, and the file of its order script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatBotFile {
    pub seat: Seat,
    pub path: PathBuf,
}

impl SeatBotFile {
    /// The bot, its script read from its file.
    pub fn read(&self) -> Result<SeatScript, OrderScriptReadError> {
        Ok(SeatScript {
            seat: self.seat,
            script: OrderScript::read(&self.path)?,
        })
    }
}

impl FromStr for SeatBotFile {
    type Err = SeatBotFileError;

    fn from_str(text: &str) -> Result<Self, SeatBotFileError> {
        let (seat, path) = text
            .split_once('=')
            .ok_or_else(|| SeatBotFileError::NotPair(text.to_owned()))?;
        let seat = seat.parse().map_err(|error| SeatBotFileError::Seat {
            text: seat.to_owned(),
            error,
        })?;
        Ok(Self {
            seat: Seat::new(seat),
            path: PathBuf::from(path),
        })
    }
}

#[cfg(test)]
mod tests;
